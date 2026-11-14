//! Jobs in parallel (T-132): up to one job for each slot at a time, each in
//! its own executor (its own units and brokers), on its own thread. The
//! source stays on the caller's thread: each line is printed when its job
//! ends, and a job that `needs` another starts after it. `max-parallel` of
//! a matrix holds the count of the running cells of one job.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use podrunner_schema::JobId;
use podrunner_source::{Event, JobSource};

use super::{End, Executor, Line, cancel_the_rest, finish, prepare, settle};
use crate::LocalSource;

/// An executor that a thread can take (an `Executor` is `Send`).
pub type Slot = Box<dyn Executor>;

/// The end of a job on its thread.
type Done = (Slot, JobId, String, Result<End, String>);

/// A cell of `job` may start: fewer of its cells run than its
/// `max-parallel`.
fn room(source: &LocalSource, job: &JobId, running: &[JobId]) -> bool {
    let Some(cell) = source.cell(job) else { return true };
    let max = cell
        .instance
        .strategy
        .as_object()
        .and_then(|o| o.get("max-parallel"))
        .and_then(|v| v.as_number())
        .filter(|n| *n >= 1.0)
        .unwrap_or(f64::INFINITY);
    let same = running.iter().filter(|r| source.cell(r).is_some_and(|c| c.job == cell.job)).count();
    (same as f64) < max
}

/// Run each job of `source` on the executors of `slots`, as many at a time
/// as there are slots. The result is true when no job failed, and the
/// slots, for their logs. When `stop` is set, no job starts; the running
/// jobs end as their executors end them (at a signal), and each job with
/// no result is `cancelled`.
pub fn drive_parallel(
    source: &mut LocalSource,
    slots: Vec<Slot>,
    out: &mut dyn FnMut(&Line),
    stop: &AtomicBool,
) -> Result<(bool, Vec<Slot>), String> {
    let mut idle = slots;
    let total = idle.len();
    let (tx, rx) = mpsc::channel::<Done>();
    let mut running: Vec<JobId> = Vec::new();
    let mut queue: VecDeque<JobId> = VecDeque::new();
    let (mut skips, mut ok) = (0, true);
    loop {
        for s in &source.skips()[skips..] {
            out(&Line::Skipped { job: s.job.clone(), condition: s.condition.clone() });
        }
        skips = source.skips().len();
        if running.is_empty() && source.is_finished() {
            // A queued cell that `fail-fast` cancelled still gets its line.
            for job in std::mem::take(&mut queue) {
                let name = source.cell(&job).map(|c| c.instance.name.clone()).unwrap_or_default();
                settle(source, &job, &name, out, &mut ok)?;
            }
            return Ok((ok, idle));
        }
        let offers = source.next(u32::MAX, Duration::ZERO).map_err(|e| e.to_string())?;
        queue.extend(offers.into_iter().filter_map(|e| match e {
            Event::Offer { offer } => Some(offer.job_id),
            _ => None,
        }));
        let stopped = stop.load(Ordering::SeqCst);
        if stopped && running.is_empty() {
            cancel_the_rest(source, skips, out);
            return Ok((false, idle));
        }
        // Start each queued job that has a slot and room.
        let mut i = 0;
        while !stopped && i < queue.len() && !idle.is_empty() {
            let job = queue[i].clone();
            let name = source.cell(&job).map(|c| c.instance.name.clone()).unwrap_or_default();
            if settle(source, &job, &name, out, &mut ok)? {
                queue.remove(i);
                continue;
            }
            if !room(source, &job, &running) {
                i += 1;
                continue;
            }
            queue.remove(i);
            let mut exec = idle.pop().expect("an idle slot");
            match prepare(source, exec.as_mut(), &job) {
                Err(reason) => {
                    finish(source, &job, name, Err(reason), out, &mut ok)?;
                    idle.push(exec);
                }
                Ok((message, limit)) => {
                    running.push(job.clone());
                    let tx = tx.clone();
                    std::thread::spawn(move || {
                        // A panic of the executor fails its job and keeps the loop.
                        let run = std::panic::AssertUnwindSafe(|| exec.run(&job, message, limit));
                        let end = std::panic::catch_unwind(run).unwrap_or_else(|_| Err("the executor panicked".into()));
                        let _ = tx.send((exec, job, name, end));
                    });
                }
            }
        }
        if running.is_empty() {
            if queue.is_empty() && !source.is_finished() {
                return Err("no job is ready, and the run is not finished".into());
            }
            continue;
        }
        // Wait for a job to end; then its slot takes the next job.
        let (exec, job, name, end) = rx.recv().map_err(|_| "a thread of a job ended with no result".to_string())?;
        running.retain(|r| r != &job);
        finish(source, &job, name, end, out, &mut ok)?;
        idle.push(exec);
        debug_assert!(idle.len() + running.len() == total);
    }
}

#[cfg(test)]
mod tests;
