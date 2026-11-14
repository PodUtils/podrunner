use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use pod_actions_workflow::{Base, JobResult};
use podrunner_broker_fake::Job;
use podrunner_schema::JobId;

use super::*;
use crate::LocalSource;

/// An executor that sleeps for each job and counts the jobs that run at
/// the same time.
struct Sleepy {
    now: Arc<AtomicUsize>,
    most: Arc<AtomicUsize>,
    fail: Option<String>,
}

impl Executor for Sleepy {
    fn run(&mut self, job: &JobId, _message: Job, _limit: Duration) -> Result<End, String> {
        let n = self.now.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(n, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(150));
        self.now.fetch_sub(1, Ordering::SeqCst);
        let result = match &self.fail {
            Some(f) if job.as_str() == f => JobResult::Failure,
            _ => JobResult::Success,
        };
        Ok(End { result, outputs: vec![] })
    }
}

fn slots(n: usize, fail: Option<&str>) -> (Vec<Slot>, Arc<AtomicUsize>) {
    let (now, most) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let v = (0..n)
        .map(|_| Box::new(Sleepy { now: now.clone(), most: most.clone(), fail: fail.map(str::to_string) }) as Slot)
        .collect();
    (v, most)
}

fn run(text: &str, n: usize, fail: Option<&str>) -> (bool, Vec<Line>, usize, Duration) {
    let mut s = LocalSource::new("l", text, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    let (slots, most) = slots(n, fail);
    let mut lines = Vec::new();
    let t = Instant::now();
    let (ok, back) = drive_parallel(&mut s, slots, &mut |l| lines.push(l.clone()), &AtomicBool::new(false)).unwrap();
    assert_eq!(back.len(), n, "each slot comes back");
    assert!(s.is_finished());
    (ok, lines, most.load(Ordering::SeqCst), t.elapsed())
}

const MATRIX: &str = "on: push\njobs:\n  m:\n    runs-on: x\n    strategy:\n      matrix:\n        k: [1, 2, 3, 4, 5, 6]\n    steps: [{run: x}]\n  after:\n    needs: m\n    runs-on: x\n    steps: [{run: x}]\n";

#[test]
fn n_jobs_run_at_a_time_and_needs_waits() {
    let (ok, lines, most, took) = run(MATRIX, 3, None);
    assert!(ok, "{lines:?}");
    assert_eq!(most, 3, "three at a time");
    assert!(took < Duration::from_millis(150 * 7), "the cells ran in parallel: {took:?}");
    assert_eq!(lines.len(), 7);
    let Line::Ran { job, .. } = lines.last().unwrap() else { panic!("{lines:?}") };
    assert_eq!(job.as_str(), "after.0", "the job of needs ends last");
    let (_, _, most, _) = run(MATRIX, 1, None);
    assert_eq!(most, 1);
}

#[test]
fn max_parallel_holds_the_cells_of_a_job() {
    let text = MATRIX.replace("    strategy:\n", "    strategy:\n      max-parallel: 2\n");
    let (ok, _, most, _) = run(&text, 4, None);
    assert!(ok);
    assert_eq!(most, 2, "max-parallel 2 with 4 slots");
}

#[test]
fn a_failed_cell_cancels_the_cells_that_did_not_start() {
    // fail-fast: m.0 fails; the cells that did not start are cancelled.
    let (ok, lines, _, _) = run(MATRIX, 1, Some("m.0"));
    assert!(!ok);
    let cancelled = lines.iter().filter(|l| matches!(l, Line::Ran { result: JobResult::Cancelled, .. })).count();
    assert_eq!(cancelled, 5, "{lines:?}");
    assert!(lines.iter().any(|l| matches!(l, Line::Skipped { job, .. } if job == "after")), "{lines:?}");
}
