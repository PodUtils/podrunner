//! The output of a step in the job container reaches the stdout of the
//! relay while the step runs (T-694), through the live route of the
//! control socket.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command as Process, Stdio};
use std::sync::{Arc, Mutex};

use podrunner_fake::FakeUnit;
use podrunner_schema::ContainerRoute;
use podrunner_unit::{StartSpec, UnitDriver, UnitId};
use serde_json::json;

use crate::policy::Policy;
use crate::server::{Unit, bind, serve};
use crate::tests::Clean;
use crate::unit::HookUnit;

const IMAGE: &str = "node@sha256:0000000000000000000000000000000000000000000000000000000000000000";

fn capability() -> podrunner_schema::Capability {
    serde_json::from_value(json!({
        "schema": 1, "kind": "fake", "arch": "x86_64", "boundary": "confined", "principal": "distinct",
        "container": "hooks", "net": "broker", "secret_delivery": ["files"], "credential_store": "private-dir",
        "mount_modes": ["rw"], "rootfs_fresh": "yes", "tree_kill": "verified", "limits": {},
        "egress_hold": "advisory", "kvm": "no"
    }))
    .unwrap()
}

/// The input of a hook call as the runner writes it: `command` first.
fn call(command: &str, response: serde_json::Value, args: serde_json::Value) -> String {
    format!(r#"{{"command":"{command}","responseFile":{response},"args":{args},"state":{{}}}}"#)
}

/// Start the relay with `input` on its stdin.
fn relay(relay: &std::path::Path, sock: &std::path::Path, input: &str) -> std::process::Child {
    let mut child = Process::new("sh")
        .arg(relay)
        .env("PODRUNNER_CTL", sock)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child
}

/// A step prints a line, then waits for a file that the test makes only
/// after it read that line, then prints again. A relay that gives the
/// output at the end of the step never gives the first line: the step
/// stops waiting at its limit and exits 99.
#[test]
fn the_output_of_a_step_comes_while_it_runs() {
    if !Process::new("curl").arg("--version").output().is_ok_and(|o| o.status.success()) {
        return; // No curl on this host; the unit image has one.
    }
    let dir = std::env::temp_dir().join(format!("podrunner-hook-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("logs")).unwrap();
    let fake: Arc<dyn UnitDriver> = Arc::new(FakeUnit::new(&dir.join("fake"), capability(), "linux/amd64"));
    let id = UnitId::parse("u-1").unwrap();
    let image = fake.prepare(IMAGE, "linux/amd64", false).unwrap();
    fake.start(&StartSpec {
        unit: id.clone(),
        image,
        argv: vec!["sleep".into(), "30".into()],
        env: Default::default(),
        credential: Default::default(),
        sockets: None,
        mounts: Vec::new(),
        limits: Default::default(),
    })
    .unwrap();
    let _clean = Clean(fake.clone(), vec![id.clone()], dir.clone());
    let policy = Policy {
        container: ContainerRoute::Hooks,
        shared_host_net: false,
        registries: vec!["docker.io".into()],
        work_root: "/home/runner".into(),
        work_dir: "_work".into(),
    };
    let hook = HookUnit::new(fake.clone(), id.clone(), policy, Some(dir.join("runner")), dir.join("logs"));
    let (sock, script) = (dir.join("ctl.sock"), dir.join("hook.sh"));
    std::fs::write(&script, crate::RELAY).unwrap();
    let listener = bind(&sock).unwrap();
    let shared: Arc<Mutex<dyn Unit>> = Arc::new(Mutex::new(hook));
    std::thread::spawn(move || serve(listener, shared));

    let response = dir.join("r.json");
    let mounts = json!([
        { "userProvidedValue": null, "sourceVolumePath": "/home/runner/_work", "targetVolumePath": "/__w", "readOnly": false }
    ]);
    let container = json!({
        "image": IMAGE, "dockerfile": null, "entryPointArgs": [], "entryPoint": "", "workingDirectory": null,
        "createOptions": null, "registry": null, "environmentVariables": {}, "portMappings": [],
        "systemMountVolumes": mounts, "userMountVolumes": [],
    });
    let prepare = call("prepare_job", json!(response), json!({ "container": container, "services": [] }));
    let o = relay(&script, &sock, &prepare).wait_with_output().unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(std::fs::read_to_string(&response).unwrap().contains("\"isAlpine\""));

    let go = dir.join("go");
    let wait = format!(
        "echo first; i=0; while [ ! -e '{g}' ] && [ $i -lt 400 ]; do sleep 0.05; i=$((i+1)); done; \
         [ -e '{g}' ] || exit 99; echo second",
        g = go.display()
    );
    let args = json!({
        "entryPoint": "sh", "entryPointArgs": ["-c", wait], "environmentVariables": { "PATH": "/usr/bin:/bin" },
    });
    let mut child = relay(&script, &sock, &call("run_script_step", json!(null), args));
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    assert_eq!(line, "first\n");
    // The event: the step goes on only now.
    std::fs::write(&go, b"").unwrap();
    let mut rest = String::new();
    out.read_to_string(&mut rest).unwrap();
    let status = child.wait().unwrap();
    let mut err = String::new();
    child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    assert_eq!((status.code(), rest.as_str()), (Some(0), "second\n"), "{err}");
    // The log of the step does not stay on the host.
    assert_eq!(std::fs::read_dir(dir.join("logs")).unwrap().count(), 0);
}
