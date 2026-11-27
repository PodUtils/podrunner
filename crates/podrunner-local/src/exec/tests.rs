use std::sync::Arc;
use std::time::Duration;

use podrunner_broker_fake::Job;
use podrunner_fake::FakeUnit;
use podrunner_schema::{Capability, JobId};
use podrunner_unit::UnitDriver;

use super::*;
use crate::drive::Executor;

pub(super) fn capability() -> Capability {
    serde_json::from_value(serde_json::json!({
        "schema": 1, "kind": "fake", "arch": "x86_64", "boundary": "confined", "principal": "distinct",
        "container": "hooks", "net": "broker", "secret_delivery": ["files"], "credential_store": "private-dir",
        "mount_modes": ["rw"], "rootfs_fresh": "yes", "tree_kill": "verified", "limits": {},
        "egress_hold": "advisory", "kvm": "no"
    }))
    .unwrap()
}

/// A top process that is not the runner gives no result: the job fails
/// with its reason, and its unit is destroyed (each unit id is new).
/// The unit `u-N` that an error of the executor names: the count of units
/// is one for the process (T-132).
fn unit_of(e: &str) -> podrunner_schema::UnitId {
    let at = e.find("u-").expect("a unit id");
    let id: String = e[at..].chars().take_while(|c| *c == '-' || *c == 'u' || c.is_ascii_digit()).collect();
    podrunner_schema::UnitId::parse(&id).unwrap()
}

#[test]
fn a_unit_with_no_result_fails_its_job_and_is_destroyed() {
    let dir = std::env::temp_dir().join(format!("podrunner-local-unit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let driver: Arc<dyn UnitDriver> = Arc::new(FakeUnit::new(&dir, capability(), "linux/amd64"));
    let image = driver.prepare(&format!("img@sha256:{}", "a".repeat(64)), "linux/amd64", false).unwrap();
    let mut exec = InUnit::new(driver.clone(), image);
    exec.run_sh = "true".into();
    exec.grace = Duration::ZERO;
    let job = JobId::new("a.0").unwrap();
    let mut ids = Vec::new();
    for _ in 1..=2 {
        let e = exec.run(&job, Job::scripts("a", &["true"]), Duration::from_secs(60)).unwrap_err();
        assert!(e.contains("no result"), "{e}");
        ids.push(unit_of(&e));
    }
    assert_ne!(ids[0], ids[1], "each job gets a new unit");
    assert!(driver.destroy(&ids[0]).unwrap().is_clean(), "destroyed before");
    assert_eq!(exec.logs.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A unit that runs past the limit of its job is ended, and its log is kept.
#[test]
fn a_unit_past_its_limit_is_ended() {
    let dir = std::env::temp_dir().join(format!("podrunner-local-limit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let driver: Arc<dyn UnitDriver> = Arc::new(FakeUnit::new(&dir, capability(), "linux/amd64"));
    let image = driver.prepare(&format!("img@sha256:{}", "a".repeat(64)), "linux/amd64", false).unwrap();
    let mut exec = InUnit::new(driver.clone(), image);
    // A runner that hangs.
    let hang = dir.with_extension("sh");
    std::fs::write(&hang, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&hang, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    exec.run_sh = hang.to_str().unwrap().into();
    exec.grace = Duration::ZERO;
    let t = std::time::Instant::now();
    let e = exec.run(&JobId::new("a.0").unwrap(), Job::scripts("a", &["true"]), Duration::from_millis(300));
    let e = e.unwrap_err();
    assert!(e.contains("timeout-minutes"), "{e}");
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
    assert_eq!(exec.logs.len(), 1);
    assert!(driver.destroy(&unit_of(&e)).unwrap().is_clean());
    let _ = std::fs::remove_file(&hang);
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-561: a runner that never reaches its broker ends the job at the bound
/// of `reach` with that cause, not at the limit of the job.
#[test]
fn a_runner_that_reaches_no_broker_ends_the_job_with_its_cause() {
    let dir = std::env::temp_dir().join(format!("podrunner-local-reach-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let driver: Arc<dyn UnitDriver> = Arc::new(FakeUnit::new(&dir, capability(), "linux/amd64"));
    let image = driver.prepare(&format!("img@sha256:{}", "a".repeat(64)), "linux/amd64", false).unwrap();
    let mut exec = InUnit::new(driver.clone(), image);
    let hang = dir.with_extension("reach.sh");
    std::fs::write(&hang, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&hang, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    exec.run_sh = hang.to_str().unwrap().into();
    exec.grace = Duration::ZERO;
    exec.reach = Duration::from_millis(300);
    let t = std::time::Instant::now();
    let e = exec.run(&JobId::new("a.0").unwrap(), Job::scripts("a", &["true"]), Duration::from_secs(60)).unwrap_err();
    assert!(e.contains("sent no request to its broker"), "{e}");
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
    assert!(driver.destroy(&unit_of(&e)).unwrap().is_clean());
    let _ = std::fs::remove_file(&hang);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_runner_directory_refuses_a_job_with_containers() {
    let dir = std::env::temp_dir().join(format!("podrunner-local-dir-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("run.sh"), "").unwrap();
    std::fs::write(dir.join("bin/Runner.Listener"), "").unwrap();
    let mut exec = RunnerDir::new(dir.clone()).unwrap();
    let mut job = Job::scripts("a", &["true"]);
    assert!(!needs_containers(&job));
    job.overrides.insert("jobContainer".into(), serde_json::json!("alpine@sha256:00"));
    let e = exec.run(&JobId::new("a.0").unwrap(), job, Duration::from_secs(60)).unwrap_err();
    assert!(e.contains("--podvm"), "{e}");
    let mut docker = Job::scripts("a", &["true"]);
    docker.overrides.insert("steps".into(), serde_json::json!([{ "reference": { "type": "containerRegistry" } }]));
    assert!(needs_containers(&docker));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_selected_guest_route_without_an_engine_refuses_a_container_job() {
    let mut exec = InUnit::new(
        Arc::new(podrunner_unit::Stub),
        podrunner_unit::Prepared { digest: format!("sha256:{}", "a".repeat(64)), platform: "linux/amd64".into() },
    );
    exec.guest = true;
    let mut job = Job::scripts("test", &["true"]);
    job.overrides.insert("jobContainer".into(), serde_json::json!("alpine"));
    let e = exec.run(&JobId::new("a.0").unwrap(), job, Duration::from_secs(30)).unwrap_err();
    assert!(e.contains("selected guest image") && e.contains("--engine DIR"), "{e}");
}

/// A second run on a runner directory that another process holds fails at
/// once (T-274); the executors of one process share the lock.
#[test]
fn a_second_run_on_a_runner_directory_fails_at_once() {
    let dir = std::env::temp_dir().join(format!("podrunner-local-lock-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("run.sh"), "").unwrap();
    std::fs::write(dir.join("bin/Runner.Listener"), "").unwrap();
    // Another process: flock(1) holds the lock file while it sleeps.
    let mut other = std::process::Command::new("flock")
        .args(["-x"])
        .arg(dir.join(".podrunner-run.lock"))
        .args(["sleep", "5"])
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let e = RunnerDir::new(dir.clone()).err().expect("the second run fails");
    assert!(e.contains("another run of podrunner"), "{e}");
    other.wait().unwrap();
    let first = RunnerDir::new(dir.clone()).unwrap();
    assert!(RunnerDir::new(dir.clone()).is_ok(), "one process shares the lock");
    drop(first);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_registries_of_the_images_of_a_job() {
    let d = "a".repeat(64);
    let mut job = Job::scripts("a", &["true"]);
    job.overrides.insert(
        "jobContainer".into(),
        serde_json::json!({ "type": 2, "map": [{ "Key": "image", "Value": format!("ghcr.io/o/i@sha256:{d}") }] }),
    );
    job.overrides.insert("jobServiceContainers".into(), serde_json::json!({ "type": 2, "map": [
        { "Key": "plain", "Value": format!("redis@sha256:{d}") },
        { "Key": "mapped", "Value": { "type": 2, "map": [{ "Key": "image", "Value": format!("localhost:5000/x@sha256:{d}") }] } },
    ] }));
    job.overrides.insert(
        "steps".into(),
        serde_json::json!([{ "reference": { "type": "containerRegistry", "image": format!("quay.io/q@sha256:{d}") } }]),
    );
    assert_eq!(images(&job).len(), 4);
    assert_eq!(registries(&job), ["docker.io", "ghcr.io", "localhost:5000", "quay.io"]);
    let mut plain = Job::scripts("a", &["true"]);
    plain.overrides.insert("jobContainer".into(), serde_json::json!(format!("alpine@sha256:{d}")));
    assert_eq!(registries(&plain), ["docker.io"]);
}

/// A stand-in runner directory whose `run.sh` runs `script`.
fn stand_in(name: &str, script: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("podrunner-local-stand-in-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("run.sh"), format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(dir.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(dir.join("bin/Runner.Listener"), "").unwrap();
    dir
}

/// T-115: a runner past the limit of its job ends with its whole process
/// group, and the job fails with the limit in the reason.
#[test]
fn a_runner_directory_ends_a_job_at_its_limit() {
    let mark = format!("podrunner-t115-{}", std::process::id());
    let dir = stand_in("limit", &format!("sh -c 'exec sleep 300 # {mark}' & wait"));
    let mut exec = RunnerDir::new(dir.clone()).unwrap();
    let t = Instant::now();
    let e = exec.run(&JobId::new("a.0").unwrap(), Job::scripts("a", &["true"]), Duration::from_secs(1)).unwrap_err();
    assert!(e.contains("limit of 1 s"), "{e}");
    assert!(t.elapsed() < Duration::from_secs(30), "the job ends at its limit");
    let left = std::fs::read_dir("/proc")
        .unwrap()
        .flatten()
        .filter_map(|p| std::fs::read(p.path().join("cmdline")).ok())
        .any(|c| String::from_utf8_lossy(&c).contains(&mark));
    assert!(!left, "no process of the runner is left");
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-115: a directory that the runner files cannot go into fails the job
/// with a reason; the command does not panic.
#[test]
fn a_runner_directory_that_takes_no_file_fails_the_job() {
    let dir = stand_in("files", "exit 0");
    // A directory where a runner file is to go: the write fails.
    std::fs::create_dir_all(dir.join(".runner")).unwrap();
    let mut exec = RunnerDir::new(dir.clone()).unwrap();
    let e = exec.run(&JobId::new("a.0").unwrap(), Job::scripts("a", &["true"]), Duration::from_secs(60)).unwrap_err();
    assert!(e.contains(".runner"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-138: `--export DIR` copies the workspace of a job, also of a job that
/// ended with no result, to `DIR/JOB`.
#[test]
fn a_runner_directory_exports_the_workspace_of_a_job() {
    let dir = stand_in("export", "mkdir -p _work/r/r _work/_temp && echo built > _work/r/r/out && touch _work/_temp/t");
    let to = dir.join("exported");
    let mut exec = RunnerDir::new(dir.clone()).unwrap();
    exec.export = Some(to.clone());
    let job = JobId::new("build.0").unwrap();
    assert!(exec.run(&job, Job::scripts("a", &["true"]), Duration::from_secs(60)).is_err());
    assert_eq!(std::fs::read_to_string(to.join("build.0/r/r/out")).unwrap(), "built\n");
    assert!(!to.join("build.0/_temp").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-138: a tool cache links its tools into `work/_tool`, then runs the
/// runner.
#[test]
fn a_tool_cache_links_into_the_cache_of_the_runner() {
    let dir = std::env::temp_dir().join(format!("podrunner-local-toolcache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let argv = crate::exec::toolcache_argv("/home/runner/run.sh", "/home/runner/work");
    assert_eq!(&argv[..2], ["/bin/sh", "-c"]);
    assert_eq!(&argv[3..], ["podrunner", "/home/runner/work/_tool", "/home/runner/run.sh"]);
    // The script on the host, with a stand-in of the mount.
    let script = argv[2].replace(crate::exec::TOOLCACHE_MOUNT, &dir.join("cache").display().to_string());
    std::fs::create_dir_all(dir.join("cache/node/20.0.0/x64")).unwrap();
    std::fs::write(dir.join("cache/node/20.0.0/x64.complete"), "").unwrap();
    let out = std::process::Command::new("/bin/sh")
        .args(["-c", &script, "podrunner", &dir.join("tool").display().to_string(), "true"])
        .status()
        .unwrap();
    assert!(out.success());
    let link = dir.join("tool/node/20.0.0/x64.complete");
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert!(dir.join("tool/node/20.0.0/x64").is_dir() && !dir.join("tool/node").is_symlink());
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-138: `export` mounts a host work directory and copies its workspaces
/// to `DIR/JOB` after the job, also of a failed job; the scratch goes.
#[test]
fn a_unit_exports_its_work_directory() {
    let dir = std::env::temp_dir().join(format!("podrunner-local-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let driver: Arc<dyn UnitDriver> = Arc::new(FakeUnit::new(&dir, capability(), "linux/amd64"));
    let image = driver.prepare(&format!("img@sha256:{}", "a".repeat(64)), "linux/amd64", false).unwrap();
    let mut exec = InUnit::new(driver, image);
    exec.run_sh = "true".into();
    exec.grace = Duration::ZERO;
    exec.export = Some(dir.join("out"));
    let job = JobId::new("build.0").unwrap();
    let e = exec.run(&job, Job::scripts("a", &["true"]), Duration::from_secs(60)).unwrap_err();
    let unit = unit_of(&e);
    assert!(dir.join("out/build.0").is_dir(), "the export of a failed job");
    let scratch = crate::runkey::in_tmp(crate::runkey::EXPORT, unit.as_str());
    assert!(!scratch.exists(), "the work directory goes");
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-225: podvm's advice to set `PODVM_STORE` becomes the advice to set
/// `TMPDIR`, the knob of the units of podrunner.
#[test]
fn a_full_store_names_tmpdir() {
    let id = podrunner_schema::UnitId::parse("u-1").unwrap();
    let e = start_error(&id, "podvm extract: not enough space. Set $PODVM_STORE to a directory with room");
    assert!(e.contains("fix: set TMPDIR"), "{e}");
    assert_eq!(start_error(&id, "other"), "start u-1: other");
}

/// T-228: a free port of the loopback, as `bind_loopback` gives it.
#[test]
fn bind_loopback_gives_a_port_that_answers() {
    let (l, port) = podrunner_broker_fake::bind_loopback().unwrap();
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_ok());
    drop(l);
}

/// A runner directory with a space in its path is refused by name (T-281).
#[test]
fn a_runner_directory_with_a_space_is_refused() {
    let dir = std::env::temp_dir().join(format!("podrunner local space {}", std::process::id()));
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("run.sh"), "").unwrap();
    std::fs::write(dir.join("bin/Runner.Listener"), "").unwrap();
    let e = RunnerDir::new(dir.clone()).err().expect("refused");
    assert!(e.contains("has a space") && e.contains("fix:"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-220: `--vnet` needs the socket volume of the driver; with none, the job
/// fails by that cause.
#[test]
fn a_vnet_unit_needs_the_socket_volume() {
    let dir = std::env::temp_dir().join(format!("podrunner-local-vnet-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(vnet::SHIM), "").unwrap();
    let driver: Arc<dyn UnitDriver> = Arc::new(FakeUnit::new(&dir.join("units"), capability(), "linux/amd64"));
    let image = driver.prepare(&format!("img@sha256:{}", "a".repeat(64)), "linux/amd64", false).unwrap();
    let mut exec = InUnit::new(driver, image);
    exec.run_sh = "true".into();
    exec.vnet = Some(dir.join(vnet::SHIM));
    let e = exec.run(&JobId::new("a.0").unwrap(), Job::scripts("a", &["true"]), Duration::from_secs(60)).unwrap_err();
    assert!(e.contains("--vnet needs the socket volume of podvm"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-331 F10: a work directory out of the runner directory goes with the
/// run.
#[test]
fn the_work_directory_out_of_the_runner_goes_with_the_run() {
    let base = std::env::temp_dir().join(format!("podrunner-local-dotwork-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let dir = base.join(".share/runner");
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("run.sh"), "").unwrap();
    std::fs::write(dir.join("bin/Runner.Listener"), "").unwrap();
    let r = RunnerDir::new(dir.clone()).unwrap();
    let work = r.work.clone();
    assert!(work.starts_with(base.join("podrunner-work")), "{}", work.display());
    std::fs::create_dir_all(work.join("x")).unwrap();
    drop(r);
    assert!(!base.join("podrunner-work").exists(), "the work directory stays");
    let _ = std::fs::remove_dir_all(&base);
}

/// A fake unit with the socket volume of podvm that keeps the spec of its
/// start and starts nothing.
pub(super) struct Mounted(pub(super) FakeUnit, pub(super) std::sync::Mutex<Option<podrunner_unit::StartSpec>>);

impl UnitDriver for Mounted {
    fn kind(&self) -> &str {
        self.0.kind()
    }
    fn probe(&self) -> Result<Capability, podrunner_unit::UnitError> {
        self.0.probe()
    }
    fn socket_mount(&self) -> Option<&str> {
        Some("/run/podrunner")
    }
    fn prepare(&self, r: &str, p: &str, e: bool) -> Result<podrunner_unit::Prepared, podrunner_unit::UnitError> {
        self.0.prepare(r, p, e)
    }
    fn start(&self, spec: &podrunner_unit::StartSpec) -> Result<(), podrunner_unit::UnitError> {
        let dir = spec.sockets.as_ref().expect("a socket directory");
        assert!(dir.join(vnet::SHIM).is_file(), "the shim is in the socket directory at the start");
        *self.1.lock().unwrap() = Some(spec.clone());
        Err(podrunner_unit::UnitError::new(podrunner_unit::UnitCode::UnitStartFailed, "kept"))
    }
    fn signal(&self, u: &podrunner_schema::UnitId, s: podrunner_unit::Signal) -> Result<(), podrunner_unit::UnitError> {
        self.0.signal(u, s)
    }
    fn wait(
        &self,
        u: &podrunner_schema::UnitId,
        t: Duration,
    ) -> Result<Option<podrunner_unit::Exit>, podrunner_unit::UnitError> {
        self.0.wait(u, t)
    }
    fn destroy(&self, u: &podrunner_schema::UnitId) -> Result<podrunner_unit::Residue, podrunner_unit::UnitError> {
        self.0.destroy(u)
    }
    fn observe(&self, u: &podrunner_schema::UnitId) -> Result<podrunner_unit::Observation, podrunner_unit::UnitError> {
        self.0.observe(u)
    }
    fn container_start(
        &self,
        u: &podrunner_schema::UnitId,
        s: &podrunner_unit::ContainerSpec,
    ) -> Result<podrunner_unit::ContainerId, podrunner_unit::UnitError> {
        self.0.container_start(u, s)
    }
    fn container_exec(
        &self,
        u: &podrunner_schema::UnitId,
        c: &podrunner_unit::ContainerId,
        s: &podrunner_unit::ExecSpec,
    ) -> Result<podrunner_unit::Exit, podrunner_unit::UnitError> {
        self.0.container_exec(u, c, s)
    }
    fn container_logs(
        &self,
        u: &podrunner_schema::UnitId,
        c: &podrunner_unit::ContainerId,
    ) -> Result<Vec<u8>, podrunner_unit::UnitError> {
        self.0.container_logs(u, c)
    }
    fn container_inspect(
        &self,
        u: &podrunner_schema::UnitId,
        c: &podrunner_unit::ContainerId,
    ) -> Result<podrunner_unit::ContainerState, podrunner_unit::UnitError> {
        self.0.container_inspect(u, c)
    }
    fn container_stop(
        &self,
        u: &podrunner_schema::UnitId,
        c: &podrunner_unit::ContainerId,
    ) -> Result<(), podrunner_unit::UnitError> {
        self.0.container_stop(u, c)
    }
}

/// T-561: on podvm's userland rung the loader of the image runs on the
/// host and opens LD_PRELOAD before the paths of the unit exist, so a shim
/// at `/run/podrunner` never loads and the runner reaches no broker. The
/// shim and the forward name the socket directory by its host path, and
/// the unit mounts it at that same path, so each rung sees one path.
#[test]
fn the_vnet_shim_and_its_forward_have_one_path_in_each_rung() {
    let dir = std::env::temp_dir().join(format!("podrunner-local-vnet-path-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(vnet::SHIM), "").unwrap();
    let driver =
        Arc::new(Mounted(FakeUnit::new(&dir.join("units"), capability(), "linux/amd64"), std::sync::Mutex::new(None)));
    let image = driver.prepare(&format!("img@sha256:{}", "a".repeat(64)), "linux/amd64", false).unwrap();
    let mut exec = InUnit::new(driver.clone(), image);
    exec.run_sh = "true".into();
    exec.vnet = Some(dir.join(vnet::SHIM));
    let e = exec.run(&JobId::new("a.0").unwrap(), Job::scripts("a", &["true"]), Duration::from_secs(60)).unwrap_err();
    assert!(e.contains("kept"), "{e}");
    let spec = driver.1.lock().unwrap().take().expect("a start");
    let sockets = spec.sockets.clone().expect("a socket directory");
    let host = sockets.to_str().unwrap();
    assert_eq!(spec.env["LD_PRELOAD"], format!("{host}/{}", vnet::SHIM));
    assert_eq!(spec.env["PODNET_VNET_FORWARD"], format!("127.0.0.1:{}=unix:{host}/egress.sock", vnet::PORT));
    assert!(spec.mounts.contains(&(sockets.clone(), host.to_string())), "{:?}", spec.mounts);
    assert!(!sockets.exists(), "the socket directory goes with the failed start");
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-564: a kind with the socket volume and no container hooks (podvm on
/// the namespace rung) refuses a job with containers before its unit
/// starts, with the fix.
#[test]
fn a_socket_volume_with_no_container_hooks_refuses_a_container_job() {
    let dir = std::env::temp_dir().join(format!("podrunner-local-nohooks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut cap = capability();
    cap.container = podrunner_schema::ContainerRoute::None;
    let driver = Arc::new(Mounted(FakeUnit::new(&dir.join("units"), cap, "linux/amd64"), std::sync::Mutex::new(None)));
    let image = driver.prepare(&format!("img@sha256:{}", "a".repeat(64)), "linux/amd64", false).unwrap();
    let mut exec = InUnit::new(driver.clone(), image);
    exec.run_sh = "true".into();
    let mut job = Job::scripts("a", &["true"]);
    job.overrides.insert("jobContainer".into(), serde_json::json!({ "image": "alpine" }));
    let e = exec.run(&JobId::new("a.0").unwrap(), job, Duration::from_secs(60)).unwrap_err();
    assert!(e.contains("no container hooks here"), "{e}");
    assert!(driver.1.lock().unwrap().is_none(), "no unit starts");
    let _ = std::fs::remove_dir_all(&dir);
}
