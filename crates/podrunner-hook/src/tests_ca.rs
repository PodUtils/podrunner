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
use crate::tests::Clean;
use crate::unit::{CA_DIR, HookUnit, UnitCa, UnitCache};

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
        externals: "externals".into(),
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
    Bundle { file, pem: PEM, vars, store: None }
}

fn started(dir: &std::path::Path) -> (Arc<dyn UnitDriver>, UnitId, Clean) {
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
    let clean = Clean(fake.clone(), vec![id.clone()], dir.to_path_buf());
    (fake, id, clean)
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
    let (fake, id, _clean) = started(&dir);
    let vars = ["SSL_CERT_FILE", "NODE_EXTRA_CA_CERTS"].map(String::from).to_vec();
    let ca = UnitCa { dir: dir.join("ca"), file: CA_FILE.into(), vars };
    let u = HookUnit::new(fake.clone(), id.clone(), policy(), Some(dir.join("runner")), dir.clone());
    let mut u = u.with_ca(Some(ca));
    let (out, spec) = step(&mut u, &fake, &id, json!({}), 0);
    // The fake maps the target to its source: the step reads the bundle.
    assert!(out.starts_with(PEM), "{out}");
    assert!(spec.contains(&format!("mount {} {CA_DIR} ro noexec\n", dir.join("ca").display())), "{spec}");
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
        assert!(spec.contains(&format!(" {CA_DIR} ro noexec\n")), "{c}: {spec}");
        // T-603: each mount of the job keeps the execute right.
        assert_eq!(spec.matches("noexec").count(), 1, "{c}: {spec}");
    }
    let _ = u.hook(parse(Command::CleanupJob, &call("cleanup_job", json!({}))).unwrap());
}

#[test]
fn a_container_of_a_unit_with_no_bundle_gets_neither() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-noca-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (fake, id, _clean) = started(&dir);
    let mut u = HookUnit::new(fake.clone(), id.clone(), policy(), Some(dir.join("runner")), dir.clone()).with_ca(None);
    let (out, spec) = step(&mut u, &fake, &id, json!({}), 0);
    assert_eq!(out, "[] []\n");
    assert!(!spec.contains(CA_DIR), "{spec}");
}

/// The hooks of a host write the bundle for the containers at mode 0644
/// (a certificate is no secret; a container user of another uid reads
/// it), out of the socket directory, and remove it at the close.
#[test]
fn the_hooks_place_the_bundle_and_remove_it_at_the_close() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-place-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sockets")).unwrap();
    let (fake, id, _clean) = started(&dir);
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
}

/// The copy keeps the name of the bundle that slots gives (the name of
/// its credential file); a name that is not a file name is refused, and
/// leaves no directory of the bundle.
#[test]
fn the_copy_keeps_the_name_that_slots_gives() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-name-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sockets")).unwrap();
    let (fake, id, _clean) = started(&dir);
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
}

/// A socket that does not bind leaves no copy of the bundle: no socket
/// serves the unit, and no close comes for it.
#[test]
fn a_socket_that_does_not_bind_leaves_no_copy_of_the_bundle() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-nobind-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (fake, id, _clean) = started(&dir);
    let hooks = Hooks::default();
    // The directory of the socket does not exist: the bind fails.
    let (socket, logs) = (dir.join("absent/ctl.sock"), dir.join("sockets.logs"));
    let vars = ["SSL_CERT_FILE".to_string()];
    assert!(hooks.open(fake.clone(), &id, policy(), &socket, &logs, Some(bundle(CA_FILE, &vars))).is_err());
    assert!(!logs.join("ca").exists(), "the copy stays after a failed bind");
    assert!(!logs.exists(), "the empty logs directory stays");
}

/// T-651: each container of a job with a cache sees the cache of the unit
/// at the same path, with the mode of the unit and its variables; a value
/// of the job keeps its priority.
#[test]
fn a_container_of_a_job_with_a_cache_gets_its_mount_and_variables() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("cache")).unwrap();
    let (fake, id, _clean) = started(&dir);
    let mut u = HookUnit::new(fake.clone(), id.clone(), policy(), Some(dir.join("runner")), dir.clone());
    let print = |env: Value| {
        let mut c = container(env);
        c["entryPointArgs"] = json!(["-c", r#"echo "[${PODRUNNER_CACHE-}] [${npm_config_cache-}]""#]);
        c
    };
    let run = |u: &mut HookUnit, env: Value| {
        let out = u.hook(parse(Command::RunContainerStep, &call("run_container_step", print(env))).unwrap()).unwrap();
        String::from_utf8(out.output).unwrap()
    };
    // No cache: no mount and no variable.
    assert_eq!(run(&mut u, json!({})), "[] []\n");
    assert!(!record(&fake, &id, "c-0").contains("/podrunner-cache"));
    for (n, read_only) in [(1, true), (2, false)] {
        let env = std::collections::BTreeMap::from([
            ("PODRUNNER_CACHE".to_string(), "/podrunner-cache".to_string()),
            ("npm_config_cache".to_string(), "/podrunner-cache/npm".to_string()),
        ]);
        u.set_cache(Some(UnitCache { dir: dir.join("cache"), target: "/podrunner-cache".into(), read_only, env }));
        let out = run(&mut u, json!({ "npm_config_cache": "/job/npm" }));
        // The fake maps the target to its source, as for the bundle.
        let host = dir.join("cache").display().to_string();
        assert_eq!(out, format!("[{host}] [/job/npm]\n"), "a value of the job keeps its priority");
        let spec = record(&fake, &id, &format!("c-{n}"));
        let mode = if read_only { "ro" } else { "rw" };
        assert!(spec.contains(&format!("mount {} /podrunner-cache {mode}\n", dir.join("cache").display())), "{spec}");
    }
}

/// T-597: a JVM reads no PEM variable. The hooks write the trust store of
/// the bundle beside it (`.p12` for `.pem`), and each container gets
/// `JAVA_TOOL_OPTIONS` that names it, before a value of the job: the JVM
/// takes the last value of a property, so the job keeps its priority.
#[test]
fn a_container_of_a_unit_with_a_bundle_gets_the_trust_store_for_a_jvm() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-jvm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (fake, id, _clean) = started(&dir);
    let vars = ["SSL_CERT_FILE".to_string()];
    let ca = crate::hooks::place_ca(&dir.join("logs"), bundle(CA_FILE, &vars)).unwrap();
    let store = dir.join("logs/ca/podrunner-broker-ca.p12");
    let der = std::fs::read(&store).unwrap();
    assert!(der.windows(9).any(|w| w == [0x04, 0x07, b'm', b'a', b'd', b'e', b'-', b'u', b'p']), "{der:02x?}");
    assert_eq!(std::fs::metadata(&store).unwrap().permissions().mode() & 0o777, 0o644);
    let mut u =
        HookUnit::new(fake.clone(), id.clone(), policy(), Some(dir.join("runner")), dir.clone()).with_ca(Some(ca));
    let java = |u: &mut HookUnit, env: Value| {
        let mut c = container(env);
        c["entryPointArgs"] = json!(["-c", r#"echo "[${JAVA_TOOL_OPTIONS-}]""#]);
        let body = call("run_container_step", c);
        String::from_utf8(u.hook(parse(Command::RunContainerStep, &body).unwrap()).unwrap().output).unwrap()
    };
    // The fake maps `CA_DIR` to its source after the `=`.
    let want = format!("-Djavax.net.ssl.trustStore={} -Djavax.net.ssl.trustStoreType=PKCS12", store.display());
    assert_eq!(java(&mut u, json!({})), format!("[{want}]\n"));
    let job = "-Djavax.net.ssl.trustStore=/job/store.p12 -Xmx1g";
    assert_eq!(java(&mut u, json!({ "JAVA_TOOL_OPTIONS": job })), format!("[{want} {job}]\n"));
    // No store beside the bundle: no option names a file that is not there.
    std::fs::remove_file(&store).unwrap();
    assert_eq!(java(&mut u, json!({})), "[]\n");
    assert_eq!(java(&mut u, json!({ "JAVA_TOOL_OPTIONS": "-Xmx1g" })), "[-Xmx1g]\n");
}

/// T-597: a bundle with no certificate gives no store (the containers keep
/// the bundle), and a stale store of the same name goes. The name of the
/// store follows the name of the bundle; a bundle named `.p12` gets none.
#[test]
fn a_bundle_that_gives_no_store_leaves_none() {
    assert_eq!(UnitCa::store_name("podrunner-ca.pem").as_deref(), Some("podrunner-ca.p12"));
    assert_eq!(UnitCa::store_name("ca").as_deref(), Some("ca.p12"));
    assert_eq!(UnitCa::store_name("ca.p12"), None);
    let dir = std::env::temp_dir().join(format!("podrunner-hook-nostore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("ca")).unwrap();
    std::fs::write(dir.join("ca/podrunner-broker-ca.p12"), "stale").unwrap();
    let vars = ["SSL_CERT_FILE".to_string()];
    let b = Bundle { file: CA_FILE, pem: "no certificate\n", vars: &vars, store: None };
    let ca = crate::hooks::place_ca(&dir, b).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("ca").join(CA_FILE)).unwrap(), "no certificate\n");
    assert!(!dir.join("ca/podrunner-broker-ca.p12").exists(), "no store, and no stale store");
    assert_eq!(ca.file, CA_FILE);
    let b = Bundle { file: "ca.p12", pem: PEM, vars: &vars, store: None };
    crate::hooks::place_ca(&dir, b).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("ca/ca.p12")).unwrap(), PEM, "the bundle, not a store");
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-693: a store that the caller gives goes to the file as it is, with
/// no second encode. The given bytes are no PKCS #12 file, so the file
/// shows who wrote it: an encode of the PEM gives other bytes. The mode and
/// the name stay the same, and a given store with no bundle name for it
/// (a bundle named `.p12`) is not written.
#[test]
fn a_given_trust_store_is_written_as_it_is() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-given-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let vars = ["SSL_CERT_FILE".to_string()];
    let given = b"given-store-not-an-encode".as_slice();
    let b = Bundle { store: Some(given), ..bundle(CA_FILE, &vars) };
    crate::hooks::place_ca(&dir, b).unwrap();
    let store = dir.join("ca/podrunner-broker-ca.p12");
    assert_eq!(std::fs::read(&store).unwrap(), given);
    assert_eq!(std::fs::metadata(&store).unwrap().permissions().mode() & 0o777, 0o644);
    assert_eq!(std::fs::read_to_string(dir.join("ca").join(CA_FILE)).unwrap(), PEM, "the bundle stays");
    // A stale store of an earlier unit is replaced by the given one.
    std::fs::write(&store, "stale").unwrap();
    crate::hooks::place_ca(&dir, Bundle { store: Some(given), ..bundle(CA_FILE, &vars) }).unwrap();
    assert_eq!(std::fs::read(&store).unwrap(), given);
    // No bytes given: the store is the encode of the PEM, as in `run`.
    crate::hooks::place_ca(&dir, bundle(CA_FILE, &vars)).unwrap();
    assert_ne!(std::fs::read(&store).unwrap(), given);
    assert!(std::fs::read(&store).unwrap().len() > given.len());
    // No store name for the bundle: the given bytes are not the bundle.
    crate::hooks::place_ca(&dir, Bundle { store: Some(given), ..bundle("ca.p12", &vars) }).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("ca/ca.p12")).unwrap(), PEM);
    let _ = std::fs::remove_dir_all(&dir);
}
