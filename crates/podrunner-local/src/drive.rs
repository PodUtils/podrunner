//! The loop of a local run (T-089): take each offer of the local source,
//! run it with an [`Executor`], and record its end, until each job is done.
//! A job that cannot become a job message (a reusable workflow, an `env:`
//! of one expression, an action or an image that is not pinned) fails with
//! its reason, as a job of GitHub fails at its setup.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use pod_actions_workflow::JobResult;
use podrunner_broker_fake::Job;
use podrunner_schema::JobId;
use podrunner_source::{Event, JobSource};

use crate::LocalSource;

/// The end of one job, as the runner reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct End {
    pub result: JobResult,
    pub outputs: Vec<(String, String)>,
}

/// Runs one job message: on the host, or in a unit. `limit` is the
/// `timeout-minutes` of the job: an executor that can end the job ends it
/// there.
pub trait Executor {
    fn run(&mut self, job: &JobId, message: Job, limit: Duration) -> Result<End, String>;
}

/// One line of the result of a run.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    /// A job ran: its offer, its name and its result.
    Ran { job: JobId, name: String, result: JobResult },
    /// A job failed before it ran, with the reason.
    Refused { job: JobId, name: String, reason: String },
    /// A job did not run: its condition, with its implicit `success() &&`.
    Skipped { job: String, condition: String },
}

/// The result of the runner's conclusion.
pub fn result_of(conclusion: &str) -> JobResult {
    match conclusion {
        "succeeded" => JobResult::Success,
        "cancelled" => JobResult::Cancelled,
        _ => JobResult::Failure,
    }
}

/// The name of the runner of each local job.
const RUNNER: &str = "podrunner-local";

/// Run each job of `source` with `exec`; give each line to `out` when it is
/// known. The result is true when no job failed. A signal stops the run
/// ([`crate::cancel`]).
pub fn drive(source: &mut LocalSource, exec: &mut dyn Executor, out: &mut dyn FnMut(&Line)) -> Result<bool, String> {
    drive_with(source, exec, out, &crate::cancel::SIGNALLED)
}

/// The skips that are not printed, then each job of the workflow with no
/// result as a `cancelled` line (T-141).
fn cancel_the_rest(source: &LocalSource, skips: usize, out: &mut dyn FnMut(&Line)) {
    for s in &source.skips()[skips..] {
        out(&Line::Skipped { job: s.job.clone(), condition: s.condition.clone() });
    }
    for j in &source.workflow().jobs {
        if source.result(&j.id).is_none() {
            let job = JobId::new(&format!("{}.0", j.id)).unwrap_or_else(|_| JobId::new("job.0").expect("a job id"));
            out(&Line::Ran { job, name: j.name.clone().unwrap_or_else(|| j.id.clone()), result: JobResult::Cancelled });
        }
    }
}

/// [`drive`] that stops when `stop` is set: the job that runs ends as its
/// executor ends it, and each job with no result is `cancelled`.
pub fn drive_with(
    source: &mut LocalSource,
    exec: &mut dyn Executor,
    out: &mut dyn FnMut(&Line),
    stop: &AtomicBool,
) -> Result<bool, String> {
    let mut skips = 0;
    let mut ok = true;
    loop {
        for s in &source.skips()[skips..] {
            out(&Line::Skipped { job: s.job.clone(), condition: s.condition.clone() });
        }
        skips = source.skips().len();
        if source.is_finished() {
            return Ok(ok);
        }
        let offers: Vec<JobId> = source
            .next(u32::MAX, Duration::ZERO)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter_map(|e| match e {
                Event::Offer { offer } => Some(offer.job_id),
                _ => None,
            })
            .collect();
        if offers.is_empty() {
            return Err("no job is ready, and the run is not finished".into());
        }
        for job in offers {
            if stop.load(Ordering::SeqCst) {
                cancel_the_rest(source, skips, out);
                return Ok(false);
            }
            let name = source.cell(&job).map(|c| c.instance.name.clone()).unwrap_or_default();
            // A cell that `fail-fast` cancelled runs no unit (T-117).
            if source.is_cancelled(&job) {
                out(&Line::Ran { job: job.clone(), name, result: JobResult::Cancelled });
                source.release(&job).map_err(|e| e.to_string())?;
                continue;
            }
            // `timeout-minutes` of the job: 360 by default, as on GitHub.
            let minutes = source.cell(&job).map(|c| c.instance.timeout_minutes).unwrap_or(360.0);
            let limit = Duration::from_secs_f64((minutes.max(0.0) * 60.0).min(360.0 * 60.0 * 24.0));
            source.ack(&job).map_err(|e| e.to_string())?;
            source.claim(&job, RUNNER).map_err(|e| e.to_string())?;
            let (end, line) = match source.broker_job(&job) {
                Err(reason) => (
                    End { result: JobResult::Failure, outputs: vec![] },
                    Line::Refused { job: job.clone(), name, reason },
                ),
                Ok(message) => match exec.run(&job, message, limit) {
                    Ok(end) => {
                        let line = Line::Ran { job: job.clone(), name, result: end.result };
                        (end, line)
                    }
                    Err(reason) => (
                        End { result: JobResult::Failure, outputs: vec![] },
                        Line::Refused { job: job.clone(), name, reason },
                    ),
                },
            };
            // A failed job with `continue-on-error` does not fail the run.
            ok &= matches!(end.result, JobResult::Success) || source.may_fail(&job);
            out(&line);
            source.complete(&job, end.result, end.outputs)?;
            source.release(&job).map_err(|e| e.to_string())?;
        }
        if stop.load(Ordering::SeqCst) {
            cancel_the_rest(source, skips, out);
            return Ok(false);
        }
    }
}

#[cfg(test)]
mod tests;
