//! T-578: each container of a unit whose broker ends TLS trusts the CA
//! bundle of that broker: a read-only mount and the variables that name
//! it. A unit with no bundle gives its containers neither.

use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use podrunner_fake::FakeUnit;
use podrunner_schema::ContainerRoute;
use podrunner_unit::{StartSpec, UnitDriver, UnitId};
use serde_json::{Value, json};

use crate::hooks::{Bundle, Hooks};
use crate::input::{Command, parse};
use crate::policy::Policy;
use crate::server::Unit;
use crate::unit::{CA_DIR, HookUnit, UnitCa};

/// The name of the bundle among the credential files of a unit.
const CA_FILE: &str = "podrunner-broker-ca.pem";
const IMAGE: &str = "node@sha256:0000000000000000000000000000000000000000000000000000000000000000";
/// A made-up certificate: the containers read it, no TLS takes it.
const PEM: &str = "-----BEGIN CERTIFICATE-----\nbWFkZS11cA==\n-----END CERTIFICATE-----\n";

fn capability() -> podrunner_schema::Capability {
    serde_json::from_value(json!({
        "schema": 1, "kind": "fake", "arch": "x86_64", "boundary": "confined", "principal": "distinct",
        "container": "hooks", "net": "broker", "secret_delivery": ["files"], "credential_store": "private-dir",
        "mount_modes": ["rw"], "rootfs_fresh": "yes", "tree_kill": "verified", "limits": {},
        "egress_hold": "advisory", "kvm": "no"
    }))
    .unwrap()
}

fn policy() -> Policy {
    Policy {
        container: ContainerRoute::Hooks,
        shared_host_net: false,
        registries: vec!["docker.io".into()],
        work_root: "/home/runner".into(),
        work_dir: "_work".into(),
    }
}

fn call(command: &str, args: Value) -> Vec<u8> {
    json!({ "command": command, "responseFile": "/home/runner/_work/_temp/r.json", "args": args, "state": {} })
        .to_string()
        .into_bytes()
}

/// A container whose entry point prints the bundle and two variables.
fn container(env: Value) -> Value {
    let script =
        r#"cat "${SSL_CERT_FILE:-/nothing}" 2>/dev/null; echo "[${NODE_EXTRA_CA_CERTS-}] [${SSL_CERT_FILE-}]""#;
    json!({
        "image": IMAGE, "entryPoint": "sh", "entryPointArgs": ["-c", script],
        "environmentVariables": env, "portMappings": [], "userMountVolumes": [],
        "systemMountVolumes": [
            { "sourceVolumePath": "/home/runner/_work", "targetVolumePath": "/github/workspace", "readOnly": false }
        ],
    })
}

fn bundle<'a>(file: &'a str, vars: &'a [String]) -> Bundle<'a> {
    Bundle { file, pem: PEM, vars }
}

fn started(dir: &std::path::Path) -> (Arc<dyn UnitDriver>, UnitId) {
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
    (fake, id)
}

/// The record of the mounts of the container `c` of the fake unit `id`.
fn record(fake: &Arc<dyn UnitDriver>, id: &UnitId, c: &str) -> String {
    let root = fake.observe(id).unwrap().unit_root.unwrap();
    std::fs::read_to_string(root.join("containers").join(c).join("spec")).unwrap()
}

/// The output of a container step of `u`, and the record of its mounts.
/// The fake names its containers `c-0`, `c-1` and on, in order.
fn step(u: &mut HookUnit, fake: &Arc<dyn UnitDriver>, id: &UnitId, env: Value, n: u32) -> (String, String) {
    let out = u.hook(parse(Command::RunContainerStep, &call("run_container_step", container(env))).unwrap()).unwrap();
    (String::from_utf8(out.output).unwrap(), record(fake, id, &format!("c-{n}")))
}

#[test]
fn a_container_of_a_unit_with_a_bundle_gets_the_mount_and_the_variables() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-ca-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("ca")).unwrap();
    std::fs::write(dir.join("ca").join(CA_FILE), PEM).unwrap();
    let (fake, id) = started(&dir);
    let vars = ["SSL_CERT_FILE", "NODE_EXTRA_CA_CERTS"].map(String::from).to_vec();
    let ca = UnitCa { dir: dir.join("ca"), file: CA_FILE.into(), vars };
    let u = HookUnit::new(fake.clone(), id.clone(), policy(), Some(dir.join("runner")), dir.clone());
    let mut u = u.with_ca(Some(ca));
    let (out, spec) = step(&mut u, &fake, &id, json!({}), 0);
    // The fake maps the target to its source: the step reads the bundle.
    assert!(out.starts_with(PEM), "{out}");
    assert!(spec.contains(&format!("mount {} {CA_DIR} ro\n", dir.join("ca").display())), "{spec}");
    // Each variable names `CA_DIR/CA_FILE`, which the fake maps to the host.
    let host = dir.join("ca").join(CA_FILE).display().to_string();
    assert!(out.ends_with(&format!("[{host}] [{host}]\n")), "{out}");
    // A value of the job keeps its priority.
    let (out, _) = step(&mut u, &fake, &id, json!({ "SSL_CERT_FILE": "/job/ca.pem" }), 1);
    assert!(out.ends_with(&format!("[{host}] [/job/ca.pem]\n")), "{out}");
    // A job container and a service get the same.
    let (mut job, mut service) = (container(json!({})), container(json!({})));
    job["systemMountVolumes"][0]["targetVolumePath"] = json!("/__w");
    service["systemMountVolumes"] = json!([]);
    job["entryPointArgs"] = json!(["-c", "sleep 30"]);
    let body = call("prepare_job", json!({ "container": job, "services": [service] }));
    let prepared = u.hook(parse(Command::PrepareJob, &body).unwrap()).unwrap();
    let response: Value = serde_json::from_slice(&prepared.output).unwrap();
    let job = response["context"]["container"]["id"].as_str().unwrap().to_string();
    let service = response["context"]["services"][0]["id"].as_str().unwrap().to_string();
    for c in [job, service] {
        let spec = record(&fake, &id, &c);
        assert!(spec.contains(&format!(" {CA_DIR} ro\n")), "{c}: {spec}");
    }
    let _ = u.hook(parse(Command::CleanupJob, &call("cleanup_job", json!({}))).unwrap());
    let _ = fake.destroy(&id);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_container_of_a_unit_with_no_bundle_gets_neither() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-noca-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (fake, id) = started(&dir);
    let mut u = HookUnit::new(fake.clone(), id.clone(), policy(), Some(dir.join("runner")), dir.clone()).with_ca(None);
    let (out, spec) = step(&mut u, &fake, &id, json!({}), 0);
    assert_eq!(out, "[] []\n");
    assert!(!spec.contains(CA_DIR), "{spec}");
    let _ = fake.destroy(&id);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The hooks of a host write the bundle for the containers at mode 0644
/// (a certificate is no secret; a container user of another uid reads
/// it), out of the socket directory, and remove it at the close.
#[test]
fn the_hooks_place_the_bundle_and_remove_it_at_the_close() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-place-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sockets")).unwrap();
    let (fake, id) = started(&dir);
    let hooks = Hooks::default();
    let (socket, logs) = (dir.join("sockets/ctl.sock"), dir.join("sockets.logs"));
    let vars = ["SSL_CERT_FILE".to_string()];
    hooks.open(fake.clone(), &id, policy(), &socket, &logs, Some(bundle(CA_FILE, &vars))).unwrap();
    let placed = logs.join("ca").join(CA_FILE);
    assert_eq!(std::fs::read_to_string(&placed).unwrap(), PEM);
    assert_eq!(std::fs::metadata(&placed).unwrap().permissions().mode() & 0o777, 0o644);
    assert_eq!(std::fs::metadata(logs.join("ca")).unwrap().permissions().mode() & 0o777, 0o755);
    assert!(!placed.starts_with(dir.join("sockets")), "out of the directory of the unit");
    hooks.close(&id);
    assert!(!logs.exists(), "the close removes the bundle and the empty logs");
    // No bundle: no directory of it.
    hooks.open(fake.clone(), &id, policy(), &socket, &logs, None).unwrap();
    assert!(!logs.join("ca").exists());
    hooks.close(&id);
    let _ = fake.destroy(&id);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The copy keeps the name of the bundle that slots gives (the name of
/// its credential file); a name that is not a file name is refused, and
/// leaves no directory of the bundle.
#[test]
fn the_copy_keeps_the_name_that_slots_gives() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-name-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sockets")).unwrap();
    let (fake, id) = started(&dir);
    let hooks = Hooks::default();
    let (socket, logs) = (dir.join("sockets/ctl.sock"), dir.join("sockets.logs"));
    let vars = ["SSL_CERT_FILE".to_string()];
    hooks.open(fake.clone(), &id, policy(), &socket, &logs, Some(bundle("other-ca.pem", &vars))).unwrap();
    assert_eq!(std::fs::read_to_string(logs.join("ca/other-ca.pem")).unwrap(), PEM);
    assert!(!logs.join("ca").join(CA_FILE).exists());
    hooks.close(&id);
    for bad in ["", ".", "..", "../x.pem", "a/b.pem"] {
        let e = hooks.open(fake.clone(), &id, policy(), &socket, &logs, Some(bundle(bad, &vars))).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput, "{bad:?}");
        assert!(!logs.exists(), "{bad:?}: no directory stays");
        assert!(!dir.join("x.pem").exists());
    }
    let _ = fake.destroy(&id);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A socket that does not bind leaves no copy of the bundle: no socket
/// serves the unit, and no close comes for it.
#[test]
fn a_socket_that_does_not_bind_leaves_no_copy_of_the_bundle() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-nobind-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (fake, id) = started(&dir);
    let hooks = Hooks::default();
    // The directory of the socket does not exist: the bind fails.
    let (socket, logs) = (dir.join("absent/ctl.sock"), dir.join("sockets.logs"));
    let vars = ["SSL_CERT_FILE".to_string()];
    assert!(hooks.open(fake.clone(), &id, policy(), &socket, &logs, Some(bundle(CA_FILE, &vars))).is_err());
    assert!(!logs.join("ca").exists(), "the copy stays after a failed bind");
    assert!(!logs.exists(), "the empty logs directory stays");
    let _ = fake.destroy(&id);
    let _ = std::fs::remove_dir_all(&dir);
}
