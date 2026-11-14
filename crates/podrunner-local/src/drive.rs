//! The loop of a local run (T-089): take each offer of the local source,
//! run it with an [`Executor`], and record its end, until each job is done.
//! A job that cannot become a job message (a reusable workflow, an action
//! or an image that is not pinned) fails with its reason, as a job of GitHub fails at its setup.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use pod_actions_workflow::JobResult;
use podrunner_broker_fake::Job;
use podrunner_schema::JobId;
use podrunner_source::{Event, JobSource};

use crate::LocalSource;

mod parallel;
pub use parallel::{Slot, drive_parallel};

/// The end of one job, as the runner reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct End {
    pub result: JobResult,
    pub outputs: Vec<(String, String)>,
}

/// Runs one job message: on the host, or in a unit. `limit` is the
/// `timeout-minutes` of the job: an executor that can end the job ends it
/// there.
pub trait Executor: Send {
    fn run(&mut self, job: &JobId, message: Job, limit: Duration) -> Result<End, String>;
    /// The labels of `runs-on` of the next job: an executor of several
    /// images takes its route here, or refuses the job (T-129).
    fn select(&mut self, _labels: &[String]) -> Result<(), String> {
        Ok(())
    }
    /// The concurrency group of the next job (T-137): an executor that holds
    /// groups holds it while the job runs.
    fn concurrency(&mut self, _group: Option<&crate::concurrency::Group>) {}
    /// The environment of the next job (T-136): an executor with an egress
    /// policy for it takes that policy.
    fn environment(&mut self, _name: Option<&str>) {}
    /// The log of each job that ran, taken once.
    fn take_logs(&mut self) -> Vec<(JobId, String)> {
        Vec::new()
    }
    /// Give each new part of a log to `live` while its job runs (T-134).
    fn set_live(&mut self, _live: crate::exec::Live) {}
    /// The stop flag of the next job (T-132): when it is set, the executor
    /// ends the job as at a signal, and its result is `cancelled`.
    fn set_stop(&mut self, _stop: crate::cancel::Stop) {}
    /// The blobs of each job that ran (its step logs and summaries), taken
    /// once (T-134).
    fn take_blobs(&mut self) -> Vec<(JobId, Vec<(String, String)>)> {
        Vec::new()
    }
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
    /// A cell that a rerun does not run: its kept result (T-135).
    Kept { job: JobId, name: String, result: JobResult },
    /// A cell that `--job` or `--matrix` keeps out (T-127): its name and why.
    NotSelected { job: JobId, name: String, reason: String },
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
            if settle(source, &job, &name, out, &mut ok)? {
                continue;
            }
            let end = prepare(source, exec, &job).and_then(|(message, limit)| exec.run(&job, message, limit));
            finish(source, &job, name, end, out, &mut ok)?;
        }
        if stop.load(Ordering::SeqCst) {
            cancel_the_rest(source, skips, out);
            return Ok(false);
        }
    }
}

/// The end of a cell that runs no job: a kept cell of a rerun (T-135), a
/// cell that the filter keeps out (T-127), a cell that `fail-fast`
/// cancelled (T-117). True when `job` is settled so.
pub(crate) fn settle(
    source: &mut LocalSource,
    job: &JobId,
    name: &str,
    out: &mut dyn FnMut(&Line),
    ok: &mut bool,
) -> Result<bool, String> {
    let name = name.to_string();
    if let Some((result, outputs)) = source.kept_of(job) {
        out(&Line::Kept { job: job.clone(), name, result });
        source.ack(job).map_err(|e| e.to_string())?;
        source.claim(job, RUNNER).map_err(|e| e.to_string())?;
        *ok &= matches!(result, JobResult::Success | JobResult::Skipped) || source.may_fail(job);
        source.complete(job, result, outputs)?;
        source.release(job).map_err(|e| e.to_string())?;
        return Ok(true);
    }
    if let Some(reason) = source.not_selected(job) {
        out(&Line::NotSelected { job: job.clone(), name, reason });
        source.ack(job).map_err(|e| e.to_string())?;
        source.claim(job, RUNNER).map_err(|e| e.to_string())?;
        source.complete(job, JobResult::Skipped, vec![])?;
        source.release(job).map_err(|e| e.to_string())?;
        return Ok(true);
    }
    if source.is_cancelled(job) {
        out(&Line::Ran { job: job.clone(), name, result: JobResult::Cancelled });
        source.release(job).map_err(|e| e.to_string())?;
        return Ok(true);
    }
    Ok(false)
}

/// Claim `job` and give `exec` its environment, group and labels; its
/// message and the limit of its `timeout-minutes` (360 by default, as on
/// GitHub).
pub(crate) fn prepare(
    source: &mut LocalSource,
    exec: &mut dyn Executor,
    job: &JobId,
) -> Result<(Job, Duration), String> {
    let minutes = source.cell(job).map(|c| c.instance.timeout_minutes).unwrap_or(360.0);
    let limit = Duration::from_secs_f64((minutes.max(0.0) * 60.0).min(360.0 * 60.0 * 24.0));
    source.ack(job).map_err(|e| e.to_string())?;
    source.claim(job, RUNNER).map_err(|e| e.to_string())?;
    let labels = source.cell(job).and_then(|c| c.instance.runs_on.as_ref()).map(|r| r.labels.clone());
    let environment = source.environment_of(job).ok().flatten();
    exec.environment(environment.as_deref());
    source
        .job_group(job)
        .map(|g| exec.concurrency(g.as_ref()))
        .and_then(|()| exec.select(&labels.unwrap_or_default()))
        .and_then(|()| source.broker_job(job))
        .map(|m| (m, limit))
}

/// Record the end of a job that ran, or that failed before it ran.
pub(crate) fn finish(
    source: &mut LocalSource,
    job: &JobId,
    name: String,
    end: Result<End, String>,
    out: &mut dyn FnMut(&Line),
    ok: &mut bool,
) -> Result<(), String> {
    let (end, line) = match end {
        Ok(end) => {
            let line = Line::Ran { job: job.clone(), name, result: end.result };
            (end, line)
        }
        Err(reason) => {
            (End { result: JobResult::Failure, outputs: vec![] }, Line::Refused { job: job.clone(), name, reason })
        }
    };
    // A failed job with `continue-on-error` does not fail the run.
    *ok &= matches!(end.result, JobResult::Success) || source.may_fail(job);
    out(&line);
    source.complete(job, end.result, end.outputs)?;
    source.release(job).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
