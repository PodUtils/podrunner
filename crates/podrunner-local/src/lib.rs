//! The `local` source (T-087): one workflow file into job offers and job
//! results, with no forge. `pod-actions-workflow` reads the file and
//! expands each job when its needs are done: a job with a matrix is one
//! offer for each cell (job id `JOB.N`), and a job whose `if:` is false
//! ends as skipped with its reason. The caller runs each offer and tells
//! its end with [`LocalSource::complete`]; the jobs after it then become
//! ready. A local set runs each job at tier `local` (call C14).

pub mod message;

use std::collections::{BTreeMap, VecDeque};
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
    workflow: &'static Workflow,
    run: Run<'static>,
    pending: VecDeque<Offer>,
    cells: BTreeMap<JobId, Cell>,
    states: BTreeMap<JobId, State>,
    /// The end of each cell of each running job.
    ends: BTreeMap<String, Vec<Option<Done>>>,
    skips: Vec<Skip>,
    vars: XValue,
    inputs: XValue,
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
        // `Run` borrows its workflow, and a source owns its run: one
        // workflow for each source lives to the end of the process.
        let workflow: &'static Workflow = Box::leak(Box::new(parsed.workflow));
        let (vars, inputs) = (base.vars.clone(), base.inputs.clone());
        let run = Run::new(workflow, base).map_err(|e| e.to_string())?;
        let mut source = LocalSource {
            name: name.into(),
            arch: arch.into(),
            workflow,
            run,
            pending: VecDeque::new(),
            cells: BTreeMap::new(),
            states: BTreeMap::new(),
            ends: BTreeMap::new(),
            skips: Vec::new(),
            vars,
            inputs,
            minter,
            reports: Vec::new(),
            audits: Vec::new(),
        };
        source.advance().map_err(|e| e.detail)?;
        Ok(source)
    }

    pub fn workflow(&self) -> &Workflow {
        self.workflow
    }

    /// The cell of an offer.
    pub fn cell(&self, job: &JobId) -> Option<&Cell> {
        self.cells.get(job)
    }

    /// The job of the fake broker for the offer `job` (T-088).
    pub fn broker_job(&self, job: &JobId) -> Result<podrunner_broker_fake::Job, String> {
        let cell = self.cells.get(job).ok_or_else(|| format!("no cell {job}"))?;
        message::broker_job(self.workflow, cell, &self.vars, &self.inputs)
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
        let ends = self.ends.get_mut(&cell.job).ok_or_else(|| format!("the job of {job} is done"))?;
        let slot = &mut ends[cell.index];
        if slot.is_some() {
            return Err(format!("{job} ended before"));
        }
        *slot = Some(Done { result, outputs });
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
                match self.run.expand(id).map_err(|e| usage(e.to_string()))? {
                    Outcome::Skipped(skip) => self.skips.push(skip),
                    Outcome::Run(instances) => {
                        self.ends.insert(id.to_string(), vec![None; instances.len()]);
                        for (index, instance) in instances.into_iter().enumerate() {
                            self.offer(id, index, instance)?;
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
