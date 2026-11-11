//! The hooks of a host with the real relay and a fake unit: the unit gets
//! the relay and the socket, a call reaches the unit, a refusal comes back
//! from `take`, and `close` removes the socket.

use std::sync::Arc;

use podrunner_fake::FakeUnit;
use podrunner_schema::{Capability, ContainerRoute, RefusalCode, UnitId};
use podrunner_slots::hooks::{HookEvent, HookPort};
use podrunner_unit::{StartSpec, UnitDriver};
use serde_json::json;

use super::HooksOf;

const IMAGE: &str = "node@sha256:0000000000000000000000000000000000000000000000000000000000000000";

fn capability() -> Capability {
    serde_json::from_value(json!({
        "schema": 1, "kind": "fake", "arch": "x86_64", "boundary": "confined", "principal": "distinct",
        "container": "hooks", "net": "broker", "secret_delivery": ["files"], "credential_store": "private-dir",
        "mount_modes": ["rw"], "rootfs_fresh": "yes", "tree_kill": "verified", "limits": {},
        "egress_hold": "advisory", "kvm": "no"
    }))
    .unwrap()
}

fn relay(env: &std::collections::BTreeMap<String, String>, input: &str) -> std::process::Output {
    use std::io::Write;
    let mut child = std::process::Command::new("sh")
        .arg(&env["ACTIONS_RUNNER_CONTAINER_HOOKS"])
        .env("PODRUNNER_CTL", &env["PODRUNNER_CTL"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn a_unit_reaches_its_socket_through_the_relay() {
    if !std::process::Command::new("curl").arg("--version").output().is_ok_and(|o| o.status.success()) {
        return; // No curl on this host; the unit image has one.
    }
    let dir = std::env::temp_dir().join(format!("podrunner-hooks-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let fake: Arc<dyn UnitDriver> = Arc::new(FakeUnit::new(&dir.join("fake"), capability(), "linux/amd64"));
    let id = UnitId::parse("u-1").unwrap();
    let image = fake.prepare(IMAGE, "linux/amd64", false).unwrap();
    let argv = vec!["sleep".into(), "30".into()];
    fake.start(&StartSpec { unit: id.clone(), image, argv, env: Default::default(), credential: Default::default() })
        .unwrap();
    let policy = podrunner_hook::policy::Policy {
        container: ContainerRoute::Hooks,
        shared_host_net: false,
        registries: vec!["docker.io".into()],
        work_root: String::new(),
    };
    let hooks = HooksOf::new([("fake".to_string(), policy)].into());
    let socket = dir.join("ctl").join("u-1.ctl");
    let env = hooks.open("fake", fake.clone(), &id, &socket).unwrap();
    assert!(env["ACTIONS_RUNNER_CONTAINER_HOOKS"].ends_with("podrunner-hook.sh"));

    // A container action that builds is refused, and the refusal comes back.
    let build = r#"{"command":"run_container_step","responseFile":null,"args":{"dockerfile":"D"},"state":null}"#;
    assert_eq!(relay(&env, build).status.code(), Some(4));
    // prepare_job with no container and no service passes.
    let prep = r#"{"command":"prepare_job","responseFile":null,"args":{"container":null,"services":[]},"state":{}}"#;
    let o = relay(&env, prep);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let (refusals, events): (Vec<RefusalCode>, Vec<HookEvent>) = hooks.take(&id);
    assert_eq!(refusals, [RefusalCode::ContainerNotSupported]);
    assert!(events.is_empty());
    // A second take is empty; close removes the socket.
    assert_eq!(hooks.take(&id).0, []);
    hooks.close(&id);
    assert!(!socket.exists());
    let _ = fake.destroy(&id);
    let _ = std::fs::remove_dir_all(&dir);
}
