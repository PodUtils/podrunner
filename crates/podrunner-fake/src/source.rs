//! The `fake` source: a script of timed events on a [`Clock`], and a canned
//! credential. `docs/fakes.md` gives the format of the script.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use podrunner_clock::Clock;
use podrunner_schema::{Credential, CredentialFile, JobId, Secret};
use podrunner_source::{Event, Hello, JobSource, Offer, SourceCode, SourceError};
use serde::Deserialize;
use serde_json::{Map, Value};

/// One line of a script.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// The time of the step on the clock, in seconds.
    pub at_s: u64,
    #[serde(flatten)]
    pub what: What,
}

/// What a step does.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum What {
    /// An offer. A second offer of one job id is a redelivery.
    Offer(Offer),
    Cancel(JobId),
    Started {
        job_id: JobId,
        runner_name: String,
    },
    Completed {
        job_id: JobId,
        result: String,
    },
    /// The next `mint` of the job fails with this code.
    MintError {
        job_id: JobId,
        code: SourceCode,
    },
}

/// Read a script: one JSON object on each line; an empty line or a line
/// that starts with `#` is skipped. The error names the line.
pub fn parse_script(text: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        steps.push(serde_json::from_str(line).map_err(|e| format!("line {}: {e}", i + 1))?);
    }
    Ok(steps)
}

/// The planted defects of the fake source: each one fails one test of the
/// source conformance suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourcePlant {
    /// Test 1: `hello` gives no name.
    NoName,
    /// Test 2: an offer with capacity 0.
    IgnoreZeroCapacity,
    /// Test 3: each offer that is due, past the capacity.
    IgnoreCapacity,
    /// Test 4: an ack of a job that was not offered.
    AckAnything,
    /// Test 5: a second claim.
    ClaimTwice,
    /// Test 6: a mint with no claim.
    MintUnclaimed,
    /// Test 7: a refused report.
    RefuseReport,
    /// Test 8: a refused audit line.
    RefuseAudit,
    /// Test 9: withdraw keeps the offer.
    KeepWithdrawn,
    /// Test 10: release keeps the claim.
    KeepReleased,
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Offered,
    Acked,
    Claimed,
}

/// The `fake` source.
pub struct FakeSource {
    name: String,
    clock: Arc<dyn Clock>,
    script: VecDeque<Step>,
    /// Offers that are due and wait for capacity.
    pending: VecDeque<Offer>,
    jobs: BTreeMap<JobId, State>,
    mint_errors: BTreeMap<JobId, SourceCode>,
    plant: Option<SourcePlant>,
    /// The report and audit lines, for the tests.
    pub reports: Vec<(JobId, Map<String, Value>)>,
    pub audits: Vec<Map<String, Value>>,
}

impl FakeSource {
    pub fn new(name: &str, clock: Arc<dyn Clock>, mut script: Vec<Step>) -> FakeSource {
        script.sort_by_key(|s| s.at_s);
        FakeSource {
            name: name.into(),
            clock,
            script: script.into(),
            pending: VecDeque::new(),
            jobs: BTreeMap::new(),
            mint_errors: BTreeMap::new(),
            plant: None,
            reports: Vec::new(),
            audits: Vec::new(),
        }
    }

    pub fn with_plant(mut self, plant: SourcePlant) -> FakeSource {
        self.plant = Some(plant);
        self
    }

    fn planted(&self, p: SourcePlant) -> bool {
        self.plant == Some(p)
    }

    /// Move each due step of the script into the state; give the events
    /// that are not offers.
    fn take_due(&mut self) -> Vec<Event> {
        let now = self.clock.now_ms() / 1000;
        let mut events = Vec::new();
        while self.script.front().is_some_and(|s| s.at_s <= now) {
            match self.script.pop_front().expect("a step").what {
                What::Offer(o) => self.pending.push_back(o),
                What::Cancel(job_id) => events.push(Event::Cancel { job_id }),
                What::Started { job_id, runner_name } => events.push(Event::Started { job_id, runner_name }),
                What::Completed { job_id, result } => events.push(Event::Completed { job_id, result }),
                What::MintError { job_id, code } => {
                    self.mint_errors.insert(job_id, code);
                }
            }
        }
        events
    }

    fn state(&self, job: &JobId) -> Result<&State, SourceError> {
        self.jobs.get(job).ok_or_else(|| SourceError::new(SourceCode::UnknownJob, format!("no offer of {job}")))
    }
}

/// The canned credential: three files of made-up text.
pub fn canned_credential(job: &JobId, runner_name: &str) -> Credential {
    let file = |name: &str, text: String| CredentialFile { name: name.into(), content: Secret::new(text) };
    Credential {
        files: vec![
            file(".runner", format!("{{\"agentName\":\"{runner_name}\",\"job\":\"{job}\"}}")),
            file(".credentials", "{\"scheme\":\"OAuth\",\"data\":{\"clientId\":\"fake-client\"}}".into()),
            file(".credentials_rsaparams", "{\"fake\":true}".into()),
        ],
    }
}

impl JobSource for FakeSource {
    fn hello(&mut self, _: &str, _: &str) -> Result<Hello, SourceError> {
        let source = if self.planted(SourcePlant::NoName) { String::new() } else { self.name.clone() };
        Ok(Hello { source, operator_controlled: true })
    }

    fn next(&mut self, capacity: u32, _wait: Duration) -> Result<Vec<Event>, SourceError> {
        let mut events = self.take_due();
        let mut room = match self.plant {
            Some(SourcePlant::IgnoreCapacity) => usize::MAX,
            Some(SourcePlant::IgnoreZeroCapacity) if capacity == 0 => 1,
            _ => capacity as usize,
        };
        while room > 0 {
            let Some(o) = self.pending.pop_front() else { break };
            self.jobs.insert(o.job_id.clone(), State::Offered);
            events.push(Event::Offer { offer: o });
            room -= 1;
        }
        Ok(events)
    }

    fn ack(&mut self, job: &JobId) -> Result<(), SourceError> {
        if self.planted(SourcePlant::AckAnything) && !self.jobs.contains_key(job) {
            return Ok(());
        }
        if *self.state(job)? == State::Offered {
            self.jobs.insert(job.clone(), State::Acked);
        }
        Ok(())
    }

    fn claim(&mut self, job: &JobId, _runner_name: &str) -> Result<(), SourceError> {
        match self.state(job)? {
            State::Claimed if !self.planted(SourcePlant::ClaimTwice) => {
                Err(SourceError::new(SourceCode::AlreadyClaimed, format!("{job} is claimed")))
            }
            _ => {
                self.jobs.insert(job.clone(), State::Claimed);
                Ok(())
            }
        }
    }

    fn mint(&mut self, job: &JobId, runner_name: &str) -> Result<Credential, SourceError> {
        if *self.state(job)? != State::Claimed && !self.planted(SourcePlant::MintUnclaimed) {
            return Err(SourceError::new(SourceCode::NotClaimed, format!("{job} is not claimed")));
        }
        if let Some(code) = self.mint_errors.remove(job) {
            return Err(SourceError::new(code, "a scripted mint error"));
        }
        Ok(canned_credential(job, runner_name))
    }

    fn report(&mut self, job: &JobId, report: &Map<String, Value>) -> Result<(), SourceError> {
        if self.planted(SourcePlant::RefuseReport) {
            return Err(SourceError::new(SourceCode::Unavailable, "planted"));
        }
        self.reports.push((job.clone(), report.clone()));
        Ok(())
    }

    fn audit(&mut self, line: &Map<String, Value>) -> Result<(), SourceError> {
        if self.planted(SourcePlant::RefuseAudit) {
            return Err(SourceError::new(SourceCode::Unavailable, "planted"));
        }
        self.audits.push(line.clone());
        Ok(())
    }

    fn withdraw(&mut self, job: &JobId, _reason: &str) -> Result<(), SourceError> {
        if *self.state(job)? == State::Claimed {
            return Err(SourceError::new(SourceCode::AlreadyClaimed, format!("{job} is claimed: release it")));
        }
        if !self.planted(SourcePlant::KeepWithdrawn) {
            self.jobs.remove(job);
        }
        Ok(())
    }

    fn release(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.state(job)?;
        if !self.planted(SourcePlant::KeepReleased) {
            self.jobs.remove(job);
        }
        Ok(())
    }
}
