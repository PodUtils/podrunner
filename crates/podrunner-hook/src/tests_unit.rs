//! The hooks of a unit on the fake unit: each command reaches the driver,
//! and a refusal reaches no driver and keeps no byte of the input.

use std::sync::Arc;

use podrunner_fake::FakeUnit;
use podrunner_schema::{Capability, ContainerRoute, RefusalCode};
use podrunner_unit::{StartSpec, UnitDriver, UnitId};
use serde_json::{Value, json};

use crate::input::{Command, parse};
use crate::policy::Policy;
use crate::server::{JobEvent, Unit};
use crate::unit::HookUnit;

const IMAGE: &str = "node@sha256:0000000000000000000000000000000000000000000000000000000000000000";
const CANARY: &str = "podrunner-canary-registry-77a0";

fn capability() -> Capability {
    serde_json::from_value(json!({
        "schema": 1, "kind": "fake", "arch": "x86_64", "boundary": "confined", "principal": "distinct",
        "container": "hooks", "net": "broker", "secret_delivery": ["files"], "credential_store": "private-dir",
        "mount_modes": ["rw"], "rootfs_fresh": "yes", "tree_kill": "verified", "limits": {},
        "egress_hold": "advisory", "kvm": "no"
    }))
    .unwrap()
}

fn call(command: &str, args: Value) -> Vec<u8> {
    json!({ "command": command, "responseFile": "/home/runner/_work/_temp/r.json", "args": args, "state": {} })
        .to_string()
        .into_bytes()
}

fn container(entry: &str, args: &[&str], target: &str) -> Value {
    json!({
        "image": IMAGE, "dockerfile": null, "entryPointArgs": args, "entryPoint": entry,
        "workingDirectory": null, "createOptions": null,
        "registry": { "username": "u", "password": CANARY, "serverUrl": "docker.io" },
        "environmentVariables": {}, "portMappings": [],
        "systemMountVolumes": [
            { "userProvidedValue": null, "sourceVolumePath": "/home/runner/_work", "targetVolumePath": target, "readOnly": false }
        ],
        "userMountVolumes": [],
    })
}

#[test]
fn each_command_reaches_the_fake_unit() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-unit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let fake: Arc<dyn UnitDriver> = Arc::new(FakeUnit::new(&dir.join("fake"), capability(), "linux/amd64"));
    let id = UnitId::parse("u-1").unwrap();
    let image = fake.prepare(IMAGE, "linux/amd64", false).unwrap();
    let spec = StartSpec {
        unit: id.clone(),
        image,
        argv: vec!["sleep".into(), "30".into()],
        env: Default::default(),
        credential: Default::default(),
    };
    fake.start(&spec).unwrap();
    let policy = Policy {
        container: ContainerRoute::Hooks,
        shared_host_net: false,
        registries: vec!["docker.io".into()],
        work_root: "/home/runner".into(),
    };
    let mut u = HookUnit::new(fake.clone(), id.clone(), policy, dir.join("runner"), dir.clone());
    let mut hook = |cmd: Command, body: Vec<u8>| u.hook(parse(cmd, &body).unwrap());

    // A step before prepare_job reaches no driver.
    let step = call("run_script_step", json!({ "entryPoint": "sh", "entryPointArgs": ["-c", "echo step; exit 3"] }));
    assert_eq!(hook(Command::RunScriptStep, step.clone()).unwrap_err().code, RefusalCode::ContainerNotSupported);

    let prepared = hook(
        Command::PrepareJob,
        call("prepare_job", json!({ "container": container("sleep", &["30"], "/__w"), "services": [] })),
    )
    .unwrap();
    let response: Value = serde_json::from_slice(&prepared.output).unwrap();
    assert_eq!(response["isAlpine"], Value::Bool(std::path::Path::new("/etc/alpine-release").exists()));
    assert!(response["context"]["container"]["id"].is_string());
    assert!(response["context"]["services"].is_array());
    assert!(!String::from_utf8_lossy(&prepared.output).contains(CANARY));
    // The mount source is the runner directory of the unit on the host.
    let cid = response["context"]["container"]["id"].as_str().unwrap().to_string();
    let root = fake.observe(&id).unwrap().unit_root.unwrap();
    let record = std::fs::read_to_string(root.join("containers").join(&cid).join("spec")).unwrap();
    assert_eq!(record, format!("mount {} /__w rw\n", dir.join("runner/_work").display()));

    let out = hook(Command::RunScriptStep, step).unwrap();
    assert_eq!((out.exit, out.output.as_slice()), (3, &b"step\n"[..]));

    let action = call("run_container_step", container("sh", &["-c", "echo action"], "/github/workspace"));
    let out = hook(Command::RunContainerStep, action).unwrap();
    assert_eq!((out.exit, out.output.as_slice()), (0, &b"action\n"[..]));

    let build = call("run_container_step", json!({ "dockerfile": "Dockerfile" }));
    let r = hook(Command::RunContainerStep, build).unwrap_err();
    assert_eq!(r.code, RefusalCode::BuildNotSupported);

    assert_eq!(hook(Command::CleanupJob, call("cleanup_job", json!({}))).unwrap().exit, 0);
    assert_eq!(u.refusals, [RefusalCode::ContainerNotSupported, RefusalCode::BuildNotSupported]);
    assert!(u.job_state(JobEvent::JobStarted).is_ok());
    assert!(u.job_state(JobEvent::JobStarted).is_err());
    assert!(u.job_state(JobEvent::JobCompleted).is_ok());
    let _ = fake.destroy(&id);
    let _ = std::fs::remove_dir_all(&dir);
}
