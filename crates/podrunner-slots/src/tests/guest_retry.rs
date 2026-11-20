//! T-574: a self-test of `guest` with no verdict tells its cause and runs
//! once more; a failed verdict does not run again.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use podrunner_fake::FakeUnit;
use podrunner_schema::{Boundary, Capability, EgressHold, Principal, UnitId};
use podrunner_unit::{
    ContainerId, ContainerSpec, ContainerState, ExecSpec, Exit, Observation, Prepared, Residue, Signal, StartSpec,
    UnitCode, UnitDriver, UnitError, UnitLog,
};

use super::*;
use crate::guest::self_test;

/// A fake unit that refuses the first `refuse` starts of the test unit, or
/// runs `script` in it in place of the test.
struct Plant {
    inner: FakeUnit,
    test: UnitId,
    refuse: u32,
    script: Option<String>,
    starts: AtomicU32,
}

impl Plant {
    fn new(dir: &TempDir, refuse: u32, script: Option<&str>) -> Plant {
        let claim = capability(Boundary::Guest, Principal::Distinct, EgressHold::Enforced);
        Plant {
            inner: FakeUnit::new(&dir.0.join("fake"), claim, "linux/amd64"),
            test: ids().1,
            refuse,
            script: script.map(str::to_string),
            starts: AtomicU32::new(0),
        }
    }

    fn run(&self, dir: &TempDir) -> (Result<(), String>, u32) {
        let verdict = self_test(self, PIN, "linux/amd64", ids(), &dir.0).map_err(|e| e.to_string());
        (verdict, self.starts.load(Ordering::SeqCst))
    }
}

fn ids() -> (UnitId, UnitId) {
    (UnitId::parse("u-40").unwrap(), UnitId::parse("u-4").unwrap())
}

impl UnitDriver for Plant {
    fn kind(&self) -> &str {
        self.inner.kind()
    }
    fn probe(&self) -> Result<Capability, UnitError> {
        self.inner.probe()
    }
    fn prepare(&self, r: &str, p: &str, e: bool) -> Result<Prepared, UnitError> {
        self.inner.prepare(r, p, e)
    }
    fn start(&self, spec: &StartSpec) -> Result<(), UnitError> {
        if spec.unit != self.test {
            return self.inner.start(spec);
        }
        if self.starts.fetch_add(1, Ordering::SeqCst) < self.refuse {
            return Err(UnitError::new(UnitCode::UnitStartFailed, "a planted refusal"));
        }
        let mut spec = spec.clone();
        if let Some(s) = &self.script {
            spec.argv = vec!["sh".into(), "-c".into(), s.clone()];
        }
        self.inner.start(&spec)
    }
    fn signal(&self, u: &UnitId, s: Signal) -> Result<(), UnitError> {
        self.inner.signal(u, s)
    }
    fn wait(&self, u: &UnitId, t: Duration) -> Result<Option<Exit>, UnitError> {
        self.inner.wait(u, t)
    }
    fn destroy(&self, u: &UnitId) -> Result<Residue, UnitError> {
        self.inner.destroy(u)
    }
    fn observe(&self, u: &UnitId) -> Result<Observation, UnitError> {
        self.inner.observe(u)
    }
    fn unit_log(&self, u: &UnitId) -> Result<UnitLog, UnitError> {
        self.inner.unit_log(u)
    }
    fn container_start(&self, u: &UnitId, s: &ContainerSpec) -> Result<ContainerId, UnitError> {
        self.inner.container_start(u, s)
    }
    fn container_exec(&self, u: &UnitId, c: &ContainerId, s: &ExecSpec) -> Result<Exit, UnitError> {
        self.inner.container_exec(u, c, s)
    }
    fn container_logs(&self, u: &UnitId, c: &ContainerId) -> Result<Vec<u8>, UnitError> {
        self.inner.container_logs(u, c)
    }
    fn container_inspect(&self, u: &UnitId, c: &ContainerId) -> Result<ContainerState, UnitError> {
        self.inner.container_inspect(u, c)
    }
    fn container_stop(&self, u: &UnitId, c: &ContainerId) -> Result<(), UnitError> {
        self.inner.container_stop(u, c)
    }
}

/// A test unit that exits with no result: the error has its exit code and
/// the end of its log, and the end stays in a file of the scratch
/// directory after the destroy.
#[test]
fn a_test_unit_with_no_result_gives_its_exit_code_and_the_end_of_its_log() {
    let dir = TempDir::new();
    let plant = Plant::new(&dir, 0, Some("echo the-cause-of-the-guest; exit 3"));
    let (verdict, _) = plant.run(&dir);
    let why = verdict.unwrap_err();
    assert!(why.contains("wrote no result"), "{why}");
    assert!(why.contains("exit code 3"), "{why}");
    assert!(why.contains("the-cause-of-the-guest"), "{why}");
    let kept = std::fs::read_to_string(dir.0.join("guest-test-fake.log")).unwrap();
    assert!(kept.contains("the-cause-of-the-guest"), "{kept}");
}

/// A test with no verdict runs once more at once; a failed verdict (a fake
/// on the kernel of the host) does not run again.
#[test]
fn a_self_test_with_no_verdict_runs_again_and_a_failed_verdict_does_not() {
    let failed = |why: &str| why.contains("boot id") || why.contains("loopback");
    // One refused start: the second try gives the verdict.
    let dir = TempDir::new();
    let (verdict, starts) = Plant::new(&dir, 1, None).run(&dir);
    let why = verdict.unwrap_err();
    assert_eq!(starts, 2, "{why}");
    assert!(failed(&why) && !why.contains("no verdict"), "{why}");
    // Two refused starts: no verdict, and no third try.
    let dir = TempDir::new();
    let (verdict, starts) = Plant::new(&dir, 2, None).run(&dir);
    let why = verdict.unwrap_err();
    assert_eq!(starts, 2, "{why}");
    assert!(why.contains("a planted refusal"), "{why}");
    // A failed verdict at once: one try.
    let dir = TempDir::new();
    let (verdict, starts) = Plant::new(&dir, 0, None).run(&dir);
    let why = verdict.unwrap_err();
    assert_eq!(starts, 1, "{why}");
    assert!(failed(&why), "{why}");
}
