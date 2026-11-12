//! The official runner in a podvm unit against the fake broker (T-026):
//! the podvm driver starts the runner image with the JIT files of the
//! broker, and the runner takes one canned job and completes it. Ignored
//! by default: it needs podvm and a store with the runner image.
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_RUNNER_IMAGE=NAME@sha256:HEX \
//!   cargo test -p podrunner-broker-fake --test podvm -- --ignored
//!
//! The image is the one of `images/runner/`, or for a test on a host with
//! no docker the stand-in of `scripts/runner-stand-in.py`. The second test
//! (T-035) runs a `container:` job through the hooks: it needs
//! `PODRUNNER_JOB_IMAGE` (NAME@sha256:HEX, an image with `sh` in the same
//! store) and podvm on the supervise rung.

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use podrunner_broker_fake::{Broker, Job, files::runner_files};
use podrunner_podvm::{Podvm, PodvmUnit};
use podrunner_schema::{Credential, CredentialFile, Secret, UnitId};
use podrunner_unit::{StartSpec, UnitDriver};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[test]
#[ignore]
fn the_runner_in_a_podvm_unit_runs_a_canned_job() {
    let state = std::env::temp_dir().join(format!("podrunner-podvm-runner-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let podvm = Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) };
    let mut unit = PodvmUnit::new(podvm, &state, &PathBuf::from(env("PODRUNNER_PODVM_IMAGES"))).unwrap();
    let image = env("PODRUNNER_RUNNER_IMAGE");
    // T-079: with PODRUNNER_PODVM_CONFINE=1 the runner runs under the
    // confine layer of podvm, proven first.
    if std::env::var_os("PODRUNNER_PODVM_CONFINE").is_some() {
        unit = unit.with_confine();
        assert!(
            unit.verify_confine(&image, UnitId::parse("u-998").unwrap()).unwrap(),
            "the confine layer is not proven"
        );
    }
    let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let prepared = unit.prepare(&image, platform, false).unwrap();

    let broker = Broker::start().unwrap();
    broker.queue(Job::scripts("in-podvm", &["echo podrunner-podvm-$((6*7))"]));
    let files = runner_files(&broker.base, "runner-u-1", &common::rsaparams())
        .into_iter()
        .map(|(name, text)| CredentialFile { name, content: Secret::new(text) })
        .collect();
    let id = UnitId::parse("u-1").unwrap();
    let spec = StartSpec {
        unit: id.clone(),
        image: prepared,
        argv: vec!["/home/runner/run.sh".into()],
        env: Default::default(),
        credential: Credential { files },
        sockets: None,
    };
    unit.start(&spec).unwrap();
    let exit = unit.wait(&id, Duration::from_secs(300)).unwrap();
    let t = Instant::now();
    while broker.completions().is_empty() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let done = broker.completions();
    let logs = broker.logs();
    // The output of the runner in the unit, for a failure.
    let unit_log = std::process::Command::new(env("PODRUNNER_PODVM"))
        .args(["logs", "u-1"])
        .env("PODVM_STORE", state.join("units").join("u-1").join("store"))
        .output()
        .map(|o| String::from_utf8_lossy(&[o.stdout, o.stderr].concat()).into_owned())
        .unwrap_or_default();
    println!("{unit_log}");
    let residue = unit.destroy(&id).unwrap();
    let _ = std::fs::remove_dir_all(&state);
    println!("exit {exit:?}, residue {residue:?}\n{:?}", broker.seen());
    assert_eq!(done.len(), 1, "{:?}\n{logs}", broker.seen());
    assert_eq!(done[0].conclusion, "succeeded", "{logs}");
    assert!(logs.contains("podrunner-podvm-42"), "{logs}");
    assert!(residue.is_clean(), "{residue:?}");
}

/// A `container:` job on the official runner in a podvm unit, through the
/// relay, the control socket in the socket directory of the unit and the
/// container verbs of the podvm driver (T-035, step 4).
#[test]
#[ignore]
fn a_container_job_runs_through_the_hooks_of_a_podvm_unit() {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    use podrunner_hook::policy::Policy;
    use podrunner_hook::server::{Unit, bind, serve_until};
    use podrunner_hook::unit::HookUnit;

    let state = std::env::temp_dir().join(format!("podrunner-podvm-hooks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let podvm = Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) };
    let mut podvm_unit = PodvmUnit::new(podvm, &state, &PathBuf::from(env("PODRUNNER_PODVM_IMAGES"))).unwrap();
    // T-079: the unit and each container under the confine layer.
    if std::env::var_os("PODRUNNER_PODVM_CONFINE").is_some() {
        podvm_unit = podvm_unit.with_confine();
        let image = env("PODRUNNER_RUNNER_IMAGE");
        assert!(podvm_unit.verify_confine(&image, UnitId::parse("u-998").unwrap()).unwrap(), "not proven");
    }
    let unit: Arc<dyn UnitDriver> = Arc::new(podvm_unit);
    let guest = unit.socket_mount().expect("podvm on the supervise rung").to_string();
    let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let prepared = unit.prepare(&env("PODRUNNER_RUNNER_IMAGE"), platform, false).unwrap();

    // The socket directory: the control socket and the relay.
    let id = UnitId::parse("u-1").unwrap();
    let sockets = state.join("sockets");
    std::fs::create_dir_all(&sockets).unwrap();
    let relay = sockets.join("podrunner-hook.sh");
    std::fs::write(&relay, podrunner_hook::RELAY).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&relay, std::fs::Permissions::from_mode(0o755)).unwrap();
    let policy = Policy {
        container: podrunner_schema::ContainerRoute::Hooks,
        shared_host_net: true,
        registries: vec!["docker.io".into()],
        work_root: "/home/runner".into(),
    };
    let hook = Arc::new(Mutex::new(HookUnit::new(unit.clone(), id.clone(), policy, None, state.join("logs"))));
    std::fs::create_dir_all(state.join("logs")).unwrap();
    let listener = bind(&sockets.join("ctl.sock")).unwrap();
    let u: Arc<Mutex<dyn Unit>> = hook.clone();
    std::thread::spawn(move || serve_until(listener, u, Arc::new(AtomicBool::new(false))));

    let broker = Broker::start().unwrap();
    let mut job =
        Job::scripts("in-container", &["echo podrunner-podvm-container-$((6*7)) > marker.txt; cat marker.txt"]);
    job.container = Some(env("PODRUNNER_JOB_IMAGE"));
    broker.queue(job);
    let files = runner_files(&broker.base, "runner-u-1", &common::rsaparams())
        .into_iter()
        .map(|(name, text)| CredentialFile { name, content: Secret::new(text) })
        .collect();
    let env_of = std::collections::BTreeMap::from([
        ("ACTIONS_RUNNER_CONTAINER_HOOKS".to_string(), format!("{guest}/podrunner-hook.sh")),
        ("PODRUNNER_CTL".to_string(), format!("{guest}/ctl.sock")),
    ]);
    let spec = StartSpec {
        unit: id.clone(),
        image: prepared,
        argv: vec!["/home/runner/run.sh".into()],
        env: env_of,
        credential: Credential { files },
        sockets: Some(sockets.clone()),
    };
    unit.start(&spec).unwrap();
    let exit = unit.wait(&id, Duration::from_secs(300)).unwrap();
    let t = Instant::now();
    while broker.completions().is_empty() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let (done, logs) = (broker.completions(), broker.logs());
    let refusals = hook.lock().unwrap().refusals.clone();
    let residue = unit.destroy(&id).unwrap();
    let _ = std::fs::remove_dir_all(&state);
    println!("exit {exit:?}, residue {residue:?}, refusals {refusals:?}\n{logs}");
    assert_eq!(done.len(), 1, "{:?}", broker.seen());
    assert_eq!(done[0].conclusion, "succeeded", "{refusals:?}\n{logs}");
    assert!(logs.contains("podrunner-podvm-container-42"), "{logs}");
    assert!(refusals.is_empty(), "{refusals:?}");
    assert!(residue.is_clean(), "{residue:?}");
}
