//! The official runner runs a job with `container:` through the container
//! hooks of podrunner: the relay script, the control socket, the checks of
//! the host and the fake unit (T-035). Ignored by default, as
//! `tests/runner.rs`.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-broker-fake --test container -- --ignored

mod common;

use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use common::run_once;
use podrunner_broker_fake::{Broker, Job};
use podrunner_fake::FakeUnit;
use podrunner_hook::policy::Policy;
use podrunner_hook::server::{Unit, bind, serve_until};
use podrunner_hook::unit::HookUnit;
use podrunner_schema::{Capability, ContainerRoute};
use podrunner_unit::{StartSpec, UnitDriver, UnitId};
use serde_json::json;

const IMAGE: &str = "busybox@sha256:0000000000000000000000000000000000000000000000000000000000000000";

fn capability() -> Capability {
    serde_json::from_value(json!({
        "schema": 1, "kind": "fake", "arch": "x86_64", "boundary": "confined", "principal": "distinct",
        "container": "hooks", "net": "broker", "secret_delivery": ["files"], "credential_store": "private-dir",
        "mount_modes": ["rw"], "rootfs_fresh": "yes", "tree_kill": "verified", "limits": {},
        "egress_hold": "advisory", "kvm": "no"
    }))
    .unwrap()
}

#[test]
#[ignore]
fn a_container_job_runs_through_the_hooks() {
    let dir = std::path::PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let tmp = std::env::temp_dir().join(format!("podrunner-container-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    // The unit, and the socket of its hooks.
    let fake: Arc<dyn UnitDriver> = Arc::new(FakeUnit::new(&tmp.join("fake"), capability(), "linux/amd64"));
    let id = UnitId::parse("u-1").unwrap();
    let image = fake.prepare(IMAGE, "linux/amd64", false).unwrap();
    let argv = vec!["sleep".into(), "600".into()];
    fake.start(&StartSpec { unit: id.clone(), image, argv, env: Default::default(), credential: Default::default() })
        .unwrap();
    let work_root = dir.display().to_string();
    let policy = Policy {
        container: ContainerRoute::Hooks,
        shared_host_net: false,
        registries: vec!["docker.io".into()],
        work_root,
    };
    let hook =
        Arc::new(Mutex::new(HookUnit::new(fake.clone(), id.clone(), policy, Some(dir.clone()), tmp.join("logs"))));
    std::fs::create_dir_all(tmp.join("logs")).unwrap();
    let socket = tmp.join("ctl.sock");
    let listener = bind(&socket).unwrap();
    let u: Arc<Mutex<dyn Unit>> = hook.clone();
    std::thread::spawn(move || serve_until(listener, u, Arc::new(AtomicBool::new(false))));
    let relay = tmp.join("podrunner-hook.sh");
    std::fs::write(&relay, podrunner_hook::RELAY).unwrap();
    std::fs::set_permissions(&relay, std::fs::Permissions::from_mode(0o755)).unwrap();

    // A step in the job container: its working directory and its paths
    // are the ones of the container (`/__w`).
    let broker = Broker::start().unwrap();
    let mut job = Job::scripts(
        "in-container",
        &["echo podrunner-container-$((6*7)) > marker.txt; echo \"workspace $GITHUB_WORKSPACE\"; pwd"],
    );
    job.container = Some(IMAGE.into());
    broker.queue(job);
    let _ = std::fs::remove_dir_all(dir.join("_work"));
    let env = [
        ("ACTIONS_RUNNER_CONTAINER_HOOKS", relay.display().to_string()),
        ("PODRUNNER_CTL", socket.display().to_string()),
    ];
    run_once(&dir, &broker, &env);
    let done = broker.completions();
    let logs = broker.logs();
    let refusals = hook.lock().unwrap().refusals.clone();
    let _ = fake.destroy(&id);
    let _ = std::fs::remove_dir_all(&tmp);
    assert_eq!(done.len(), 1, "{:?}\n{logs}", broker.seen());
    assert_eq!(done[0].conclusion, "succeeded", "{refusals:?}\n{logs}");
    assert!(refusals.is_empty(), "{refusals:?}");
    // The step wrote through the mount of `/__w`. The runner gave the path
    // of the container (`/__w/app/app`); the fake maps it to its source,
    // so the step prints the host path.
    let marker = std::fs::read_to_string(dir.join("_work/app/app/marker.txt")).unwrap();
    assert_eq!(marker.trim(), "podrunner-container-42");
    let host = dir.join("_work/app/app").display().to_string();
    assert!(logs.contains(&format!("workspace {host}")), "{logs}");
}
