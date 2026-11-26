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
use crate::tests::Clean;
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
        sockets: None,
        mounts: Vec::new(),
        limits: Default::default(),
    };
    fake.start(&spec).unwrap();
    let _clean = Clean(fake.clone(), vec![id.clone()], dir.clone());
    let policy = Policy {
        container: ContainerRoute::Hooks,
        shared_host_net: false,
        registries: vec!["docker.io".into()],
        work_root: "/home/runner".into(),
        work_dir: "_work".into(),
    };
    let mut u = HookUnit::new(fake.clone(), id.clone(), policy, Some(dir.join("runner")), dir.clone());
    let mut hook = |cmd: Command, body: Vec<u8>| u.hook(parse(cmd, &body).unwrap());

    // A step before prepare_job reaches no driver.
    let step = call("run_script_step", json!({ "entryPoint": "sh", "entryPointArgs": ["-c", "echo step; exit 3"] }));
    assert_eq!(hook(Command::RunScriptStep, step.clone()).unwrap_err().code, RefusalCode::ContainerNotSupported);

    let prepared = hook(
        Command::PrepareJob,
        call("prepare_job", json!({ "container": container("", &[], "/__w"), "services": [] })),
    )
    .unwrap();
    let response: Value = serde_json::from_slice(&prepared.output).unwrap();
    assert_eq!(response["isAlpine"], Value::Bool(std::path::Path::new("/etc/alpine-release").exists()));
    assert!(response["context"]["container"]["id"].is_string());
    assert!(response["context"]["services"].is_array());
    assert!(!String::from_utf8_lossy(&prepared.output).contains(CANARY));
    // The mount source is the runner directory of the unit on the host.
    let cid = response["context"]["container"]["id"].as_str().unwrap().to_string();
    // The runner gives the job container no entry point: it still lives.
    let state = fake.container_inspect(&id, &podrunner_unit::ContainerId(cid.clone())).unwrap();
    assert!(state.running, "{state:?}");
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
    assert_eq!(u.refusals[..2], [RefusalCode::ContainerNotSupported, RefusalCode::BuildNotSupported]);
    // The job container and the action had one image: its digest, once.
    assert_eq!(u.images, [IMAGE.rsplit_once('@').unwrap().1]);
    assert!(u.job_state(JobEvent::JobStarted).is_ok());
    // A second job-started is ignored and goes to the audit as a refusal.
    assert!(u.job_state(JobEvent::JobStarted).is_ok());
    assert_eq!(u.events.iter().filter(|e| **e == JobEvent::JobStarted).count(), 1);
    assert!(u.job_state(JobEvent::JobCompleted).is_ok());
}

/// The design test 9 of the unit contract (T-053): no call names a
/// container id, so the socket of unit B reaches no container of unit A.
#[test]
fn the_socket_of_one_unit_reaches_no_container_of_another() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-two-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let fake: Arc<dyn UnitDriver> = Arc::new(FakeUnit::new(&dir.join("fake"), capability(), "linux/amd64"));
    let policy = || Policy {
        container: ContainerRoute::Hooks,
        shared_host_net: false,
        registries: vec!["docker.io".into()],
        work_root: "/home/runner".into(),
        work_dir: "_work".into(),
    };
    let ids = ["u-1", "u-2"].map(|n| UnitId::parse(n).unwrap());
    let _clean = Clean(fake.clone(), ids.to_vec(), dir.clone());
    let mut units = Vec::new();
    for name in ["u-1", "u-2"] {
        let id = UnitId::parse(name).unwrap();
        let image = fake.prepare(IMAGE, "linux/amd64", false).unwrap();
        let argv = vec!["sleep".into(), "30".into()];
        fake.start(&StartSpec {
            unit: id.clone(),
            image,
            argv,
            env: Default::default(),
            credential: Default::default(),
            sockets: None,
            mounts: Vec::new(),
            limits: Default::default(),
        })
        .unwrap();
        units.push((id.clone(), HookUnit::new(fake.clone(), id, policy(), Some(dir.join("runner")), dir.clone())));
    }
    let job = call("prepare_job", json!({ "container": container("", &[], "/__w"), "services": [] }));
    units[0].1.hook(parse(Command::PrepareJob, &job).unwrap()).unwrap();
    // The step of unit B finds no job container: it never sees the one of A.
    let step = call("run_script_step", json!({ "entryPoint": "sh", "entryPointArgs": ["-c", "true"] }));
    let r = units[1].1.hook(parse(Command::RunScriptStep, &step).unwrap()).unwrap_err();
    assert_eq!(r.code, RefusalCode::ContainerNotSupported);
    // The containers of A are under the root of A only.
    let root_b = fake.observe(&units[1].0).unwrap().unit_root.unwrap();
    assert!(!root_b.join("containers").exists());
}

/// T-403: a mount source with a symbolic link in the tree of the unit is
/// refused: a job that makes `_work` a link to the host gets no container.
#[test]
fn a_linked_mount_source_is_refused() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-link-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("runner")).unwrap();
    std::fs::create_dir_all(dir.join("host")).unwrap();
    std::os::unix::fs::symlink(dir.join("host"), dir.join("runner/_work")).unwrap();
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
    let mut u = HookUnit::new(fake.clone(), id.clone(), policy.clone(), Some(dir.join("runner")), dir.clone());
    let body = call("prepare_job", json!({ "container": container("", &[], "/__w"), "services": [] }));
    let e = u.hook(parse(Command::PrepareJob, &body).unwrap()).unwrap_err();
    assert_eq!(e.code, RefusalCode::ContainerOptionRefused, "{e:?}");
    // T-590: the same for the hosted work folder `work`: a link in it, or in
    // a part of it, is refused.
    std::fs::create_dir_all(dir.join("runner/work")).unwrap();
    std::os::unix::fs::symlink(dir.join("host"), dir.join("runner/work/_temp")).unwrap();
    let hosted = Policy { work_dir: "work".into(), ..policy };
    for (source, refused) in [("/home/runner/work", false), ("/home/runner/work/_temp/_github_home", true)] {
        let mut u = HookUnit::new(fake.clone(), id.clone(), hosted.clone(), Some(dir.join("runner")), dir.clone());
        let mut c = container("", &[], "/__w");
        c["systemMountVolumes"][0]["sourceVolumePath"] = json!(source);
        let body = call("prepare_job", json!({ "container": c, "services": [] }));
        let r = u.hook(parse(Command::PrepareJob, &body).unwrap());
        assert_eq!(
            r.as_ref().err().map(|e| e.code),
            refused.then_some(RefusalCode::ContainerOptionRefused),
            "{source}"
        );
        let _ = u.hook(parse(Command::CleanupJob, &call("cleanup_job", json!({}))).unwrap());
    }
}

/// #278: a job container that stops at its start fails `prepare_job` with
/// a cause that names what a job container needs, as on GitHub (`tail`
/// and `sh`); it does not fail later at the first step.
#[test]
fn a_job_container_that_stops_at_its_start_is_named() {
    let dir = std::env::temp_dir().join(format!("podrunner-hook-stops-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
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
        sockets: None,
        mounts: Vec::new(),
        limits: Default::default(),
    };
    fake.start(&spec).unwrap();
    let _clean = Clean(fake.clone(), vec![id.clone()], dir.clone());
    let policy = Policy {
        container: ContainerRoute::Hooks,
        shared_host_net: false,
        registries: vec!["docker.io".into()],
        work_root: "/home/runner".into(),
        work_dir: "_work".into(),
    };
    let mut u = HookUnit::new(fake.clone(), id.clone(), policy, Some(dir.join("runner")), dir.clone());
    let body = call("prepare_job", json!({ "container": container("sh", &["-c", "exit 9"], "/__w"), "services": [] }));
    let e = u.hook(parse(Command::PrepareJob, &body).unwrap()).unwrap_err();
    assert!(e.why.contains("it stopped (at its start or later)") && e.why.contains("`tail` and `sh`"), "{e:?}");
    assert!(e.why.contains("job container"), "{e:?}");
    assert!(u.driver_errors.iter().any(|d| d.contains("UnitStartFailed")), "{:?}", u.driver_errors);
}

/// The parent of `pid`, from `/proc/PID/stat`.
fn parent(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()
}

/// T-703: a test of a fake unit that fails leaves no process of the unit
/// (the payload, a container, their shims), and no such process holds a
/// descriptor of the test that has no `FD_CLOEXEC`, as the lock of `flock`
/// that cargo holds. The planted failure is a panic after the start; the
/// guard `Clean` destroys the unit.
#[test]
fn a_failed_test_of_a_fake_unit_leaves_no_process_and_no_descriptor() {
    use std::os::fd::AsRawFd;
    let dir = std::env::temp_dir().join(format!("podrunner-hook-planted-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let lock = std::fs::File::create(dir.join("cargo.lock")).unwrap();
    // SAFETY: fcntl on a descriptor that `lock` owns.
    assert_eq!(unsafe { libc::fcntl(lock.as_raw_fd(), libc::F_SETFD, 0) }, 0);
    let target = std::fs::read_link(format!("/proc/self/fd/{}", lock.as_raw_fd())).unwrap();
    let tag = podrunner_unit::procs::tag_of(&dir.join("fake"), "u-1");
    let seen = std::sync::Mutex::new((Vec::new(), Vec::new()));
    // The serve thread of a control socket holds the driver past a panic,
    // as in `tests_live`: only the guard ends the unit.
    let held: std::sync::Mutex<Option<Arc<dyn UnitDriver>>> = std::sync::Mutex::new(None);
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let fake: Arc<dyn UnitDriver> = Arc::new(FakeUnit::new(&dir.join("fake"), capability(), "linux/amd64"));
        let id = UnitId::parse("u-1").unwrap();
        let image = fake.prepare(IMAGE, "linux/amd64", false).unwrap();
        let spec = StartSpec {
            unit: id.clone(),
            image,
            argv: vec!["sleep".into(), "30".into()],
            env: Default::default(),
            credential: Default::default(),
            sockets: None,
            mounts: Vec::new(),
            limits: Default::default(),
        };
        *held.lock().unwrap() = Some(fake.clone());
        fake.start(&spec).unwrap();
        let _clean = Clean(fake.clone(), vec![id.clone()], dir.join("fake"));
        let policy = Policy {
            container: ContainerRoute::Hooks,
            shared_host_net: false,
            registries: vec!["docker.io".into()],
            work_root: "/home/runner".into(),
            work_dir: "_work".into(),
        };
        let mut u = HookUnit::new(fake.clone(), id, policy, Some(dir.join("runner")), dir.clone());
        let body = call("prepare_job", json!({ "container": container("", &[], "/__w"), "services": [] }));
        u.hook(parse(Command::PrepareJob, &body).unwrap()).unwrap();
        // The payload, the job container and the shim of each.
        let mut pids = podrunner_unit::shim::tree_of_tagged(&tag);
        pids.extend(pids.clone().into_iter().filter_map(parent).filter(|p| *p != std::process::id()));
        let holders: Vec<u32> = pids
            .iter()
            .copied()
            .filter(|p| {
                std::fs::read_dir(format!("/proc/{p}/fd"))
                    .into_iter()
                    .flatten()
                    .flatten()
                    .any(|fd| std::fs::read_link(fd.path()).is_ok_and(|t| t == target))
            })
            .collect();
        *seen.lock().unwrap() = (pids, holders);
        panic!("the planted failure");
    }));
    assert!(failed.is_err());
    let (pids, holders) = seen.into_inner().unwrap();
    assert!(pids.len() >= 4, "the payload, the container and their shims: {pids:?}");
    assert!(holders.is_empty(), "these processes of the unit hold the lock: {holders:?}");
    let left: Vec<u32> = pids.into_iter().filter(|p| podrunner_unit::conformance::alive(*p)).collect();
    assert!(left.is_empty(), "the unit left {left:?}");
    drop(held);
    drop(lock);
    let _ = std::fs::remove_dir_all(&dir);
}
