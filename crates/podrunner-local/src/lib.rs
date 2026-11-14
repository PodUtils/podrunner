//! The `local` source (T-087): one workflow file into job offers and job
//! results, with no forge. `pod-actions-workflow` reads the file and
//! expands each job when its needs are done: a job with a matrix is one
//! offer for each cell (job id `JOB.N`), and a job whose `if:` is false
//! ends as skipped with its reason. The caller runs each offer and tells
//! its end with [`LocalSource::complete`]; the jobs after it then become
//! ready. A local set runs each job at tier `local` (call C14).

pub mod cancel;
pub mod dispatch;
pub mod drive;
pub mod exec;
pub mod front;
pub mod images;
pub mod lock;
pub mod message;
pub mod secrets;
pub mod store;
pub mod tree;
pub mod work;

/// The artifact store of a run (T-119).
pub use podrunner_broker_fake::artifacts;
/// The cache store (T-120).
pub use podrunner_broker_fake::cache;

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use pod_actions_expr::Value as XValue;
use pod_actions_workflow::{Base, Done, Instance, JobResult, Outcome, Run, Skip, Workflow};
use podrunner_schema::{Credential, JobId};
use podrunner_source::{Event, Hello, JobSource, Offer, SourceCode, SourceError};
use serde_json::{Map, Value, json};

/// The credential of a claimed job: the caller makes it (T-089: the files
/// of the fake broker for this job).
pub type Minter = Box<dyn FnMut(&JobId, &str) -> Credential + Send>;

/// The wait deadline of a local offer: a local run has no other consumer.
const WAIT_S: u64 = 24 * 3600;

/// One offer: a cell of a job.
#[derive(Debug, Clone)]
pub struct Cell {
    /// The id of the job in the workflow.
    pub job: String,
    /// The index of the cell in the job.
    pub index: usize,
    pub instance: Instance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Offered,
    Acked,
    Claimed,
}

/// The `local` source.
pub struct LocalSource {
    name: String,
    arch: String,
    run: Run<Arc<Workflow>>,
    pending: VecDeque<Offer>,
    cells: BTreeMap<JobId, Cell>,
    states: BTreeMap<JobId, State>,
    /// The end of each cell of each running job.
    ends: BTreeMap<String, Vec<Option<Done>>>,
    skips: Vec<Skip>,
    /// The `github` context of the run: each step reads it (T-114).
    github: XValue,
    /// The cells that `fail-fast` cancelled before they ran (T-117).
    cancelled: std::collections::BTreeSet<JobId>,
    vars: XValue,
    inputs: XValue,
    /// The secrets of the run (T-121) and the token of `github.token`
    /// (T-122).
    secrets: Vec<(String, String)>,
    token: Option<String>,
    /// The commit of each `OWNER/REPO@REF` of the run (T-118, T-133).
    pins: BTreeMap<String, String>,
    /// The jobs that `--job` selects with their needs, and the values of
    /// `--matrix` (T-127).
    selected: Option<std::collections::BTreeSet<String>>,
    matrix_filter: Vec<(String, String)>,
    minter: Minter,
    /// The report and audit lines, for the caller.
    pub reports: Vec<(JobId, Map<String, Value>)>,
    pub audits: Vec<Map<String, Value>>,
}

fn usage(detail: impl Into<String>) -> SourceError {
    SourceError::new(SourceCode::BadRequest, detail)
}

impl LocalSource {
    /// A source of the workflow `text`. `base` gives the contexts `github`,
    /// `vars` and `inputs`; `arch` is the architecture of each offer.
    pub fn new(name: &str, text: &str, base: Base, arch: &str, minter: Minter) -> Result<LocalSource, String> {
        let parsed = pod_actions_workflow::parse(text).map_err(|e| e.to_string())?;
        let (github, vars, inputs) = (base.github.clone(), base.vars.clone(), base.inputs.clone());
        let run = Run::new(Arc::new(parsed.workflow), base).map_err(|e| e.to_string())?;
        let mut source = LocalSource {
            name: name.into(),
            arch: arch.into(),
            run,
            pending: VecDeque::new(),
            cells: BTreeMap::new(),
            states: BTreeMap::new(),
            ends: BTreeMap::new(),
            skips: Vec::new(),
            github,
            cancelled: Default::default(),
            vars,
            inputs,
            secrets: Vec::new(),
            token: None,
            pins: BTreeMap::new(),
            selected: None,
            matrix_filter: Vec::new(),
            minter,
            reports: Vec::new(),
            audits: Vec::new(),
        };
        source.advance().map_err(|e| e.detail)?;
        Ok(source)
    }

    /// Give each job the secrets of the run (T-121), and `token` as
    /// `github.token` to each job whose `permissions` are not `{}` (T-122).
    pub fn set_secrets(&mut self, secrets: Vec<(String, String)>, token: Option<String>) {
        self.secrets = secrets;
        self.token = token;
    }

    /// The commit of each `OWNER/REPO@REF` of the run: a checkout of another
    /// repository takes its tree from the store (T-133).
    pub fn set_pins(&mut self, pins: BTreeMap<String, String>) {
        self.pins = pins;
    }

    /// Run only `jobs` and each job that they need, and only the cells whose
    /// matrix has each value of `matrix` (T-127). A job that the workflow
    /// does not have is refused by name.
    pub fn set_filter(&mut self, jobs: &[String], matrix: &[(String, String)]) -> Result<(), String> {
        if !jobs.is_empty() {
            let mut set = std::collections::BTreeSet::new();
            for j in jobs {
                if self.run.workflow().job(j).is_none() {
                    let ids: Vec<&str> = self.run.workflow().jobs.iter().map(|j| j.id.as_str()).collect();
                    return Err(format!("--job {j}: the workflow has no such job (its jobs: {ids:?})"));
                }
                set.insert(j.clone());
                set.extend(self.run.graph().ancestors(j).into_iter().map(str::to_string));
            }
            self.selected = Some(set);
        }
        self.matrix_filter = matrix.to_vec();
        Ok(())
    }

    /// Why the filter keeps a cell out (T-127), or `None`.
    pub fn not_selected(&self, job: &JobId) -> Option<String> {
        let cell = self.cells.get(job)?;
        if let Some(set) = &self.selected
            && !set.contains(&cell.job)
        {
            return Some("not selected (--job)".into());
        }
        for (k, want) in &self.matrix_filter {
            let got = match cell.instance.matrix.get(k) {
                None => continue,
                Some(XValue::String(s)) => s.clone(),
                Some(XValue::Bool(b)) => b.to_string(),
                Some(XValue::Number(n)) if n.fract() == 0.0 => format!("{n:.0}"),
                Some(XValue::Number(n)) => n.to_string(),
                Some(other) => other.to_json_line(),
            };
            if &got != want {
                return Some(format!("not selected (--matrix {k}={want}; the cell has {got})"));
            }
        }
        None
    }

    /// `fail-fast` cancelled the cell `job` before it ran (T-117).
    pub fn is_cancelled(&self, job: &JobId) -> bool {
        self.cancelled.contains(job)
    }

    /// The job of the cell `job` has `continue-on-error`: its failure does
    /// not fail the run (T-117).
    pub fn may_fail(&self, job: &JobId) -> bool {
        self.cells.get(job).is_some_and(|c| c.instance.continue_on_error)
    }

    pub fn workflow(&self) -> &Workflow {
        self.run.workflow()
    }

    /// The cell of an offer.
    pub fn cell(&self, job: &JobId) -> Option<&Cell> {
        self.cells.get(job)
    }

    /// The job of the fake broker for the offer `job` (T-088).
    pub fn broker_job(&self, job: &JobId) -> Result<podrunner_broker_fake::Job, String> {
        let cell = self.cells.get(job).ok_or_else(|| format!("no cell {job}"))?;
        let mut b = message::broker_job(self.run.workflow(), cell, &self.github, &self.vars, &self.inputs, &self.pins)?;
        b.secrets = self.secrets.clone();
        let none = |p: Option<&pod_actions_workflow::Node>| matches!(p.map(|n| &n.kind), Some(pod_actions_workflow::Kind::Map(e)) if e.is_empty());
        let w = self.run.workflow();
        let permissions = w.job(&cell.job).and_then(|j| j.permissions.as_ref()).or(w.permissions.as_ref());
        b.token = if none(permissions) { None } else { self.token.clone() };
        Ok(b)
    }

    /// The jobs that did not run, with their reasons, in their order.
    pub fn skips(&self) -> &[Skip] {
        &self.skips
    }

    /// The result of a job that is done.
    pub fn result(&self, job: &str) -> Option<JobResult> {
        self.run.result(job)
    }

    /// Each job is done.
    pub fn is_finished(&self) -> bool {
        self.run.is_finished()
    }

    /// The end of the cell `job`: its result and its outputs. When each
    /// cell of its job ended, the job is done with the combined result
    /// (the outputs of the last cell, as GitHub keeps one set of outputs
    /// for a matrix), and the jobs after it can start.
    pub fn complete(&mut self, job: &JobId, result: JobResult, outputs: Vec<(String, String)>) -> Result<(), String> {
        let cell = self.cells.get(job).ok_or_else(|| format!("no cell {job}"))?;
        // A failed cell of a job with `continue-on-error` does not fail the
        // run: `needs` gives the jobs after it a success (T-117).
        let allowed = cell.instance.continue_on_error;
        let result = if result == JobResult::Failure && allowed { JobResult::Success } else { result };
        let fail_fast = result == JobResult::Failure
            && cell.instance.strategy.as_object().and_then(|o| o.get("fail-fast")).and_then(|v| v.as_bool())
                == Some(true);
        let (job_name, index) = (cell.job.clone(), cell.index);
        let ends = self.ends.get_mut(&job_name).ok_or_else(|| format!("the job of {job} is done"))?;
        let slot = &mut ends[index];
        if slot.is_some() {
            return Err(format!("{job} ended before"));
        }
        *slot = Some(Done { result, outputs });
        // `fail-fast`: each cell of the job that did not end is cancelled,
        // as GitHub cancels the cells in progress and in the queue (T-117).
        if fail_fast {
            for (i, s) in ends.iter_mut().enumerate() {
                if s.is_none() {
                    *s = Some(Done::of(JobResult::Cancelled));
                    if let Ok(id) = JobId::new(&format!("{job_name}.{i}")) {
                        self.pending.retain(|o| o.job_id != id);
                        self.cancelled.insert(id);
                    }
                }
            }
        }
        let cell = self.cells.get(job).expect("the cell");
        if ends.iter().all(Option::is_some) {
            let job_id = cell.job.clone();
            let ends = self.ends.remove(&job_id).expect("the ends of the job");
            let results: Vec<JobResult> = ends.iter().flatten().map(|d| d.result).collect();
            let outputs = ends.into_iter().flatten().next_back().map(|d| d.outputs).unwrap_or_default();
            let done = Done { result: JobResult::combine(results), outputs };
            self.run.finish(&job_id, done).map_err(|e| e.to_string())?;
            self.advance().map_err(|e| e.detail)?;
        }
        Ok(())
    }

    /// Expand each ready job, until none is ready: a skip can make the
    /// next job ready.
    fn advance(&mut self) -> Result<(), SourceError> {
        loop {
            let ready = self.run.ready();
            if ready.is_empty() {
                return Ok(());
            }
            for id in ready {
                match self.run.expand(&id).map_err(|e| usage(e.to_string()))? {
                    Outcome::Skipped(skip) => self.skips.push(skip),
                    Outcome::Run(instances) => {
                        self.ends.insert(id.clone(), vec![None; instances.len()]);
                        for (index, instance) in instances.into_iter().enumerate() {
                            self.offer(&id, index, instance)?;
                        }
                    }
                }
            }
        }
    }

    fn offer(&mut self, id: &str, index: usize, instance: Instance) -> Result<(), SourceError> {
        let job_id = JobId::new(&format!("{id}.{index}")).map_err(|e| usage(format!("job id of {id}: {e:?}")))?;
        let labels = instance.runs_on.as_ref().map(|r| r.labels.clone()).unwrap_or_default();
        let mut fields = Map::new();
        fields.insert("source".into(), json!(self.name));
        fields.insert("labels".into(), json!(labels));
        fields.insert("arch".into(), json!(self.arch));
        fields.insert("wait_deadline_s".into(), json!(WAIT_S));
        fields.insert("name".into(), json!(instance.name));
        self.pending.push_back(Offer { job_id: job_id.clone(), fields });
        self.cells.insert(job_id, Cell { job: id.to_string(), index, instance });
        Ok(())
    }

    fn state(&self, job: &JobId) -> Result<State, SourceError> {
        self.states
            .get(job)
            .copied()
            .ok_or_else(|| SourceError::new(SourceCode::UnknownJob, format!("no offer of {job}")))
    }
}

impl JobSource for LocalSource {
    fn hello(&mut self, _: &str, _: &str) -> Result<Hello, SourceError> {
        Ok(Hello { source: self.name.clone(), operator_controlled: true })
    }

    fn next(&mut self, capacity: u32, _wait: Duration) -> Result<Vec<Event>, SourceError> {
        let mut events = Vec::new();
        for _ in 0..capacity {
            let Some(offer) = self.pending.pop_front() else { break };
            self.states.insert(offer.job_id.clone(), State::Offered);
            events.push(Event::Offer { offer });
        }
        Ok(events)
    }

    fn ack(&mut self, job: &JobId) -> Result<(), SourceError> {
        if self.state(job)? == State::Offered {
            self.states.insert(job.clone(), State::Acked);
        }
        Ok(())
    }

    fn claim(&mut self, job: &JobId, _runner_name: &str) -> Result<(), SourceError> {
        if self.state(job)? == State::Claimed {
            return Err(SourceError::new(SourceCode::AlreadyClaimed, format!("{job} is claimed")));
        }
        self.states.insert(job.clone(), State::Claimed);
        Ok(())
    }

    fn mint(&mut self, job: &JobId, runner_name: &str) -> Result<Credential, SourceError> {
        if self.state(job)? != State::Claimed {
            return Err(SourceError::new(SourceCode::NotClaimed, format!("{job} is not claimed")));
        }
        Ok((self.minter)(job, runner_name))
    }

    fn report(&mut self, job: &JobId, report: &Map<String, Value>) -> Result<(), SourceError> {
        self.reports.push((job.clone(), report.clone()));
        Ok(())
    }

    fn audit(&mut self, line: &Map<String, Value>) -> Result<(), SourceError> {
        self.audits.push(line.clone());
        Ok(())
    }

    fn withdraw(&mut self, job: &JobId, _reason: &str) -> Result<(), SourceError> {
        if self.state(job)? == State::Claimed {
            return Err(SourceError::new(SourceCode::AlreadyClaimed, format!("{job} is claimed: release it")));
        }
        self.states.remove(job);
        Ok(())
    }

    fn release(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.state(job)?;
        self.states.remove(job);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
