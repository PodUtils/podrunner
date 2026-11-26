//! T-685: a kind can measure the disk of a unit itself
//! (`UnitDriver::disk_bytes`), as the incus kind must for the root disk of
//! an instance: the host cannot walk those files. The disk worker adds the
//! measure to the walk of the unit root, off the tick.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use podrunner_schema::{Boundary, Capability, EgressHold, ExitReason, Principal, Residual, UnitId};
use podrunner_unit::{
    ContainerId, ContainerSpec, ContainerState, ExecSpec, Exit, Observation, Prepared, Residue, Signal, StartSpec,
    UnitDriver, UnitError, UnitLog,
};
use serde_json::json;

use super::*;
use crate::audit;
use crate::disk::CHECK_MS;

/// A fake unit whose kind measures `own` bytes, and whose root is `root`
/// (`None`: the kind gives no root).
struct Measured {
    inner: Arc<dyn UnitDriver>,
    own: Option<u64>,
    root: Option<PathBuf>,
    calls: Arc<AtomicU32>,
}

impl UnitDriver for Measured {
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
        self.inner.start(spec)
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
        Ok(Observation { unit_root: self.root.clone(), ..self.inner.observe(u)? })
    }
    fn disk_bytes(&self, _: &UnitId) -> Option<u64> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.own
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

/// A bed with one running unit of a kind that measures `own` bytes, a
/// root that holds one file of `file` bytes (`None`: no root), and the
/// disk bound `bound`; the clock is at the first disk check.
fn bed(own: Option<u64>, file: Option<u64>, bound: u64) -> (Bed, Arc<AtomicU32>) {
    let lines = vec![
        offer(0, "j-1", json!({})),
        format!(r#"{{"at_s":{},"started":{{"job_id":"j-1","runner_name":"podrunner-host-a-u-0"}}}}"#, CHECK_MS / 1000),
    ];
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, lines)],
        vec![KindSpec { name: "fake", slots: 1, capability: cap, command: sh("exec sleep 300") }],
    );
    let root = file.map(|n| {
        let root = bed.dir.0.join("measured-root");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("file"), vec![0; usize::try_from(n).unwrap()]).unwrap();
        root
    });
    let calls = Arc::new(AtomicU32::new(0));
    let kind = &mut bed.slots.kinds[0];
    kind.driver = Arc::new(Measured { inner: kind.driver.clone(), own, root, calls: calls.clone() });
    kind.cfg.disk_max_bytes = Some(bound);
    bed.until(10, |s| s.jobs().any(|j| j.state == State::Started));
    bed.clock.advance(Duration::from_millis(CHECK_MS - bed.slots.now()));
    (bed, calls)
}

/// The peak of the one sample of the bed.
fn peak(own: Option<u64>, file: Option<u64>) -> Option<u64> {
    let (mut bed, calls) = bed(own, file, 1 << 40);
    bed.slots.tick();
    bed.until(0, |s| s.disk_worker.pending == 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let job = bed.slots.jobs().next().unwrap();
    assert_eq!(job.term_ms, None, "below the bound the unit runs");
    job.times.disk_peak
}

#[test]
fn the_measure_of_the_kind_adds_to_the_walk_of_the_root() {
    assert_eq!(peak(Some(1000), Some(5000)), Some(6000));
    // No measure of the kind: the walk alone holds.
    assert_eq!(peak(None, Some(5000)), Some(5000));
    // No root: the measure of the kind alone holds.
    assert_eq!(peak(Some(1000), None), Some(1000));
    // Neither: no sample.
    assert_eq!(peak(None, None), None);
}

/// A unit whose kind measures more than the bound ends with `limit`, also
/// with no root for a walk.
#[test]
fn a_unit_above_its_bound_by_the_measure_of_its_kind_ends_with_limit() {
    for file in [None, Some(10)] {
        let (mut bed, _) = bed(Some(9000), file, 8192);
        bed.until(0, Bed::all_final);
        let r = &bed.slots.reports[0];
        assert_eq!(r.exit_reason, ExitReason::Limit, "{r:?}");
        assert!(matches!(r.disk_peak_bytes, Some(Residual::Count(9000))), "{r:?}");
        let limits: Vec<_> = bed.slots.audit_lines().iter().filter(|a| a.audit == audit::Kind::Limit).collect();
        assert_eq!(limits.len(), 1);
        assert_eq!(limits[0].detail.as_deref(), Some("disk"));
    }
}

/// The measure of the kind runs on the thread of the walks, not on the
/// tick (T-652): a held worker holds no tick and calls no driver.
#[test]
fn the_measure_of_the_kind_runs_off_the_tick() {
    let (mut bed, calls) = bed(Some(9000), None, 8192);
    let gate = bed.slots.disk_worker.gate.clone();
    let held = gate.lock().unwrap();
    for _ in 0..3 {
        bed.slots.tick();
        assert_eq!(bed.slots.disk_worker.pending, 1);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(bed.slots.jobs().next().unwrap().term_ms, None);
        bed.clock.advance(Duration::from_millis(CHECK_MS));
    }
    drop(held);
    bed.until(0, Bed::all_final);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Limit);
}
