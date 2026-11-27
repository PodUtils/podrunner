//! T-639: a cancel of a job whose container step blocks ends within the
//! bound of a plain job: the unit goes with its containers before the hooks
//! stop, so the stop does not wait for the step. A hook call that the first
//! destroy does not end gets a line of the log, and a second destroy.
//! T-720, T-721: each destroy at a cancel holds the job only until the
//! bound.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use pod_actions_workflow::JobResult;
use podrunner_broker_fake::Job;
use podrunner_fake::FakeUnit;
use podrunner_schema::{JobId, UnitId};
use podrunner_unit::{ContainerId, ContainerSpec, ContainerState, ExecSpec, Exit, UnitDriver, UnitError};

use super::InUnit;
use crate::drive::Executor;

/// The fake unit with a socket mount, so that the job gets its hooks. It
/// counts its destroys, and it can hold each `container_start` for a time
/// that no destroy shortens (a call in `podvm run -d`).
struct Hooked {
    unit: FakeUnit,
    destroys: AtomicUsize,
    hold: Option<(Duration, Arc<AtomicBool>)>,
    /// Each destroy waits for this flag (at most 20 s), then counts its end.
    slow: Option<(Arc<AtomicBool>, Arc<AtomicUsize>)>,
    /// The unit ends when this flag is set: at the cancel (T-721).
    ends_at: Option<Arc<AtomicBool>>,
}

impl Hooked {
    fn new(unit: FakeUnit) -> Hooked {
        Hooked { unit, destroys: AtomicUsize::new(0), hold: None, slow: None, ends_at: None }
    }
}

impl UnitDriver for Hooked {
    fn kind(&self) -> &str {
        self.unit.kind()
    }
    fn probe(&self) -> Result<podrunner_schema::Capability, UnitError> {
        self.unit.probe()
    }
    fn socket_mount(&self) -> Option<&str> {
        Some("/run/podrunner")
    }
    fn prepare(&self, r: &str, p: &str, e: bool) -> Result<podrunner_unit::Prepared, UnitError> {
        self.unit.prepare(r, p, e)
    }
    fn start(&self, spec: &podrunner_unit::StartSpec) -> Result<(), UnitError> {
        self.unit.start(spec)
    }
    fn signal(&self, u: &UnitId, s: podrunner_unit::Signal) -> Result<(), UnitError> {
        self.unit.signal(u, s)
    }
    fn wait(&self, u: &UnitId, t: Duration) -> Result<Option<Exit>, UnitError> {
        match &self.ends_at {
            Some(at) if at.load(Ordering::SeqCst) => Ok(Some(Exit::Code(143))),
            _ => self.unit.wait(u, t),
        }
    }
    fn destroy(&self, u: &UnitId) -> Result<podrunner_unit::Residue, UnitError> {
        self.destroys.fetch_add(1, Ordering::SeqCst);
        if let Some((go, ended)) = &self.slow {
            let t = Instant::now();
            while !go.load(Ordering::SeqCst) && t.elapsed() < Duration::from_secs(20) {
                std::thread::sleep(Duration::from_millis(10));
            }
            let r = self.unit.destroy(u);
            ended.fetch_add(1, Ordering::SeqCst);
            return r;
        }
        self.unit.destroy(u)
    }
    fn observe(&self, u: &UnitId) -> Result<podrunner_unit::Observation, UnitError> {
        self.unit.observe(u)
    }
    fn container_start(&self, u: &UnitId, s: &ContainerSpec) -> Result<ContainerId, UnitError> {
        if let Some((time, entered)) = &self.hold {
            entered.store(true, Ordering::SeqCst);
            std::thread::sleep(*time);
            return Err(UnitError::new(podrunner_unit::UnitCode::UnitStartFailed, "held"));
        }
        self.unit.container_start(u, s)
    }
    fn container_exec(&self, u: &UnitId, c: &ContainerId, s: &ExecSpec) -> Result<Exit, UnitError> {
        self.unit.container_exec(u, c, s)
    }
    fn container_logs(&self, u: &UnitId, c: &ContainerId) -> Result<Vec<u8>, UnitError> {
        self.unit.container_logs(u, c)
    }
    fn container_inspect(&self, u: &UnitId, c: &ContainerId) -> Result<ContainerState, UnitError> {
        self.unit.container_inspect(u, c)
    }
    fn container_stop(&self, u: &UnitId, c: &ContainerId) -> Result<(), UnitError> {
        self.unit.container_stop(u, c)
    }
}

/// Each process of the step: `sh -c "sleep 60; : MARK"` (not the curl
/// that posts it, whose argument holds the same text in JSON).
fn with_mark(mark: &str) -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        if let Ok(c) = std::fs::read(e.path().join("cmdline"))
            && String::from_utf8_lossy(&c).contains(&format!("-c\0sleep 60; : {mark}"))
        {
            out.push(e.file_name().to_string_lossy().into_owned());
        }
    }
    out
}

/// A job with one container step that blocks: the runner of the unit posts
/// it to the control socket of the unit as the relay posts it. The cancel
/// comes when `running()` says that the step runs; the answer is the
/// executor, its job, the flag of the stop and the mark of the step.
fn blocked(dir: &std::path::Path, driver: Arc<dyn UnitDriver>, mark: &str) -> (InUnit, Job, Arc<AtomicBool>) {
    let image = format!("node@sha256:{}", "0".repeat(64));
    let prepared = driver.prepare(&image, "linux/amd64", false).unwrap();
    let mut exec = InUnit::new(driver, prepared);
    let step = serde_json::json!({
        "command": "run_container_step", "responseFile": "", "state": {},
        "args": {
            "image": image, "entryPoint": "sh", "entryPointArgs": ["-c", format!("sleep 60; : {mark}")],
            "environmentVariables": {}, "portMappings": [], "userMountVolumes": [], "systemMountVolumes": [],
        },
    });
    let runner = dir.join("run.sh");
    let socket = crate::runkey::in_tmp(crate::runkey::UNIT_DIR, "");
    std::fs::write(
        &runner,
        format!(
            "#!/bin/sh\ncurl -sS --unix-socket '{}'\"$PODRUNNER_UNIT\"/ctl.sock -H 'Content-Type: application/json' \
             --data-binary '{step}' http://unit/v1/hook/run_container_step\n",
            socket.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&runner, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    exec.run_sh = runner.display().to_string();
    let stop = Arc::new(AtomicBool::new(false));
    exec.set_stop(stop.clone());
    let mut job = Job::scripts("a", &["true"]);
    job.overrides.insert("jobContainer".into(), serde_json::json!(image));
    (exec, job, stop)
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("podrunner-local-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_cancel_ends_a_job_whose_container_step_blocks() {
    let dir = scratch("cancel-step");
    let driver: Arc<dyn UnitDriver> =
        Arc::new(Hooked::new(FakeUnit::new(&dir.join("units"), super::tests::capability(), "linux/amd64")));
    let mark = format!("podrunner-t639-{}", std::process::id());
    let (mut exec, job, stop) = blocked(&dir, driver, &mark);
    // The cancel comes when the step runs.
    let (m, s) = (mark.clone(), stop.clone());
    let cancel = std::thread::spawn(move || {
        let t = Instant::now();
        while with_mark(&m).is_empty() && t.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let running = !with_mark(&m).is_empty();
        s.store(true, Ordering::SeqCst);
        running
    });
    let t = Instant::now();
    let end = exec.run(&JobId::new("a.0").unwrap(), job, Duration::from_secs(600));
    let took = t.elapsed();
    assert!(cancel.join().unwrap(), "the container step ran");
    assert!(matches!(end, Ok(ref e) if matches!(e.result, JobResult::Cancelled)), "{:?}", end.map(|e| e.result));
    // The bound of a plain job: the slice of the wait and the end of the
    // unit; the step alone would take 60 s.
    assert!(took < Duration::from_secs(30), "{took:?}");
    assert!(with_mark(&mark).is_empty(), "the step of the container ended: {:?}", with_mark(&mark));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A hook call that no destroy ends (it is in `podvm run -d`): the log says
/// that the run did not wait for it, with no "refused" in front, and the
/// log says that the refusals of the call are not in it. The unit is
/// destroyed once more after the stop of the hooks, as the call can start a
/// container after the first destroy.
#[test]
fn a_hook_call_that_no_destroy_ends_is_named_and_the_unit_is_destroyed_again() {
    let dir = scratch("cancel-held");
    let entered = Arc::new(AtomicBool::new(false));
    let mut hooked = Hooked::new(FakeUnit::new(&dir.join("units"), super::tests::capability(), "linux/amd64"));
    hooked.hold = Some((Duration::from_secs(4), entered.clone()));
    let hooked = Arc::new(hooked);
    let (mut exec, job, stop) = blocked(&dir, hooked.clone(), "podrunner-t639-held");
    let cancel = {
        let entered = entered.clone();
        std::thread::spawn(move || {
            let t = Instant::now();
            while !entered.load(Ordering::SeqCst) && t.elapsed() < Duration::from_secs(20) {
                std::thread::sleep(Duration::from_millis(50));
            }
            stop.store(true, Ordering::SeqCst);
        })
    };
    let end = exec.run(&JobId::new("a.0").unwrap(), job, Duration::from_secs(600));
    cancel.join().unwrap();
    assert!(matches!(end, Ok(ref e) if matches!(e.result, JobResult::Cancelled)), "{:?}", end.map(|e| e.result));
    let log = &exec.logs[0].1;
    assert!(log.contains("\npodrunner: a hook call did not end in 1 s; the run does not wait for it"), "{log}");
    assert!(!log.contains("refused: a hook call did not end"), "{log}");
    assert!(log.contains("\npodrunner: a hook call still holds the unit: "), "{log}");
    assert_eq!(hooked.destroys.load(Ordering::SeqCst), 2, "one destroy before the stop of the hooks, one after");
    // The held call ends: nothing of the job writes in the directory.
    std::thread::sleep(Duration::from_secs(4));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The executor, the flag of the stop, the flag `go`, and the count of the
/// destroys that ended.
type SlowEnd = (InUnit, Arc<AtomicBool>, Arc<AtomicBool>, Arc<AtomicUsize>);

/// A job that runs until a cancel, on a fake unit whose destroy waits for
/// the flag `go`, with a bound of 200 ms. With `ends_at_cancel`, the unit
/// ends when the stop flag is set.
fn slow_end(dir: &std::path::Path, ends_at_cancel: bool) -> SlowEnd {
    let mut hooked = Hooked::new(FakeUnit::new(&dir.join("units"), super::tests::capability(), "linux/amd64"));
    let (go, ended) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicUsize::new(0)));
    let stop = Arc::new(AtomicBool::new(false));
    hooked.slow = Some((go.clone(), ended.clone()));
    hooked.ends_at = ends_at_cancel.then(|| stop.clone());
    let driver: Arc<dyn UnitDriver> = Arc::new(hooked);
    let prepared = driver.prepare(&format!("node@sha256:{}", "0".repeat(64)), "linux/amd64", false).unwrap();
    let mut exec = InUnit::new(driver, prepared);
    let hang = dir.join("hang.sh");
    std::fs::write(&hang, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&hang, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    exec.run_sh = hang.display().to_string();
    exec.grace = Duration::ZERO;
    exec.bound = Duration::from_millis(200);
    exec.set_stop(stop.clone());
    (exec, stop, go, ended)
}

/// Let each held destroy end, and wait for its end (an event).
fn release(go: &AtomicBool, ended: &AtomicUsize) {
    go.store(true, Ordering::SeqCst);
    let t = Instant::now();
    while ended.load(Ordering::SeqCst) == 0 && t.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(ended.load(Ordering::SeqCst), 1, "the destroy ended in the background");
}

/// T-720: a stop of the job that is not a signal (`fail-fast`) with a unit
/// that does not go: the job ends `cancelled` at the bound, its log names
/// the unit, and the destroy goes on in the background.
#[test]
fn a_stop_by_fail_fast_waits_for_a_unit_only_until_the_bound() {
    let dir = scratch("cancel-bound");
    let (mut exec, stop, go, ended) = slow_end(&dir, false);
    stop.store(true, Ordering::SeqCst);
    let t = Instant::now();
    let end = exec.run(&JobId::new("a.0").unwrap(), Job::scripts("a", &["true"]), Duration::from_secs(600));
    let took = t.elapsed();
    assert!(matches!(end, Ok(ref e) if matches!(e.result, JobResult::Cancelled)), "{:?}", end.map(|e| e.result));
    assert!(took < Duration::from_secs(10), "the job waited for its unit: {took:?}");
    assert_eq!(ended.load(Ordering::SeqCst), 0, "the destroy still goes");
    let log = &exec.logs[0].1;
    assert!(log.contains(" did not go in 0 s at the cancel; its end goes on in the background"), "{log}");
    release(&go, &ended);
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-721: a unit that ends at the cancel: its destroy has the bound too,
/// and the job fails with the cause at the bound.
#[test]
fn a_destroy_of_a_unit_that_ended_at_the_cancel_has_the_bound() {
    let dir = scratch("cancel-ended");
    let (mut exec, stop, go, ended) = slow_end(&dir, true);
    stop.store(true, Ordering::SeqCst);
    let t = Instant::now();
    let end = exec.run(&JobId::new("a.0").unwrap(), Job::scripts("a", &["true"]), Duration::from_secs(600));
    let took = t.elapsed();
    assert!(took < Duration::from_secs(10), "the job waited for its unit: {took:?}");
    let e = end.map(|e| e.result).unwrap_err();
    assert!(e.starts_with("unit u-") && e.ends_with(" did not go in 0 s at the cancel"), "{e}");
    assert_eq!(ended.load(Ordering::SeqCst), 0, "the destroy still goes");
    assert!(exec.logs[0].1.contains(" did not go in 0 s at the cancel; its end goes on in the background"));
    release(&go, &ended);
    let _ = std::fs::remove_dir_all(&dir);
}
