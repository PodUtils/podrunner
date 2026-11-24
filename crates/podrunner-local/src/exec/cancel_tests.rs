//! T-639: a cancel of a job whose container step blocks ends within the
//! bound of a plain job: the unit goes with its containers before the hooks
//! stop, so the stop does not wait for the step.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use pod_actions_workflow::JobResult;
use podrunner_broker_fake::Job;
use podrunner_fake::FakeUnit;
use podrunner_schema::{JobId, UnitId};
use podrunner_unit::{ContainerId, ContainerSpec, ContainerState, ExecSpec, Exit, UnitDriver, UnitError};

use super::InUnit;
use crate::drive::Executor;

/// The fake unit with a socket mount, so that the job gets its hooks.
struct Hooked(FakeUnit);

impl UnitDriver for Hooked {
    fn kind(&self) -> &str {
        self.0.kind()
    }
    fn probe(&self) -> Result<podrunner_schema::Capability, UnitError> {
        self.0.probe()
    }
    fn socket_mount(&self) -> Option<&str> {
        Some("/run/podrunner")
    }
    fn prepare(&self, r: &str, p: &str, e: bool) -> Result<podrunner_unit::Prepared, UnitError> {
        self.0.prepare(r, p, e)
    }
    fn start(&self, spec: &podrunner_unit::StartSpec) -> Result<(), UnitError> {
        self.0.start(spec)
    }
    fn signal(&self, u: &UnitId, s: podrunner_unit::Signal) -> Result<(), UnitError> {
        self.0.signal(u, s)
    }
    fn wait(&self, u: &UnitId, t: Duration) -> Result<Option<Exit>, UnitError> {
        self.0.wait(u, t)
    }
    fn destroy(&self, u: &UnitId) -> Result<podrunner_unit::Residue, UnitError> {
        self.0.destroy(u)
    }
    fn observe(&self, u: &UnitId) -> Result<podrunner_unit::Observation, UnitError> {
        self.0.observe(u)
    }
    fn container_start(&self, u: &UnitId, s: &ContainerSpec) -> Result<ContainerId, UnitError> {
        self.0.container_start(u, s)
    }
    fn container_exec(&self, u: &UnitId, c: &ContainerId, s: &ExecSpec) -> Result<Exit, UnitError> {
        self.0.container_exec(u, c, s)
    }
    fn container_logs(&self, u: &UnitId, c: &ContainerId) -> Result<Vec<u8>, UnitError> {
        self.0.container_logs(u, c)
    }
    fn container_inspect(&self, u: &UnitId, c: &ContainerId) -> Result<ContainerState, UnitError> {
        self.0.container_inspect(u, c)
    }
    fn container_stop(&self, u: &UnitId, c: &ContainerId) -> Result<(), UnitError> {
        self.0.container_stop(u, c)
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

#[test]
fn a_cancel_ends_a_job_whose_container_step_blocks() {
    let dir = std::env::temp_dir().join(format!("podrunner-local-cancel-step-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let image = format!("node@sha256:{}", "0".repeat(64));
    let driver: Arc<dyn UnitDriver> =
        Arc::new(Hooked(FakeUnit::new(&dir.join("units"), super::tests::capability(), "linux/amd64")));
    let prepared = driver.prepare(&image, "linux/amd64", false).unwrap();
    let mut exec = InUnit::new(driver.clone(), prepared);
    // The runner of the unit: one container step that blocks, posted to
    // the control socket of the unit as the relay posts it.
    let mark = format!("podrunner-t639-{}", std::process::id());
    let step = serde_json::json!({
        "command": "run_container_step", "responseFile": "", "state": {},
        "args": {
            "image": image, "entryPoint": "sh", "entryPointArgs": ["-c", format!("sleep 60; : {mark}")],
            "environmentVariables": {}, "portMappings": [], "userMountVolumes": [], "systemMountVolumes": [],
        },
    });
    let runner = dir.join("run.sh");
    let socket = std::env::temp_dir().join(format!("podrunner-local-unit-{}-", std::process::id()));
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
