//! The supervisor: one `tick` moves each job one step on, from the events
//! of the sources, the clock and the units. The intake of offers is here;
//! the life of a unit is in `run.rs`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use podrunner_clock::Clock;
use podrunner_schema::{Capability, ExitReason, JobId, JobOffer, RefusalCode, Tier, UnitIds};
use podrunner_source::{Event, JobSource, Offer, SourceCode};
use podrunner_unit::UnitDriver;
use serde_json::Value;

use crate::audit::{self, Audit, Entry};
use crate::config::{Config, SourceConfig, SourceKind, UnitConfig};
use crate::journal::Journal;
use crate::placement::{self, Kind, Placement};
use crate::state::{Line, State};

/// One source instance.
pub struct Source {
    pub cfg: SourceConfig,
    pub source: Box<dyn JobSource>,
    /// The source revoked podrunner: it gets no capacity.
    pub revoked: bool,
}

/// One unit kind of the host.
pub struct UnitKind {
    pub name: String,
    pub cfg: UnitConfig,
    pub driver: Box<dyn UnitDriver>,
    pub capability: Capability,
}

/// One job that podrunner knows.
#[derive(Debug, Clone)]
pub struct Job {
    pub source: usize,
    pub id: JobId,
    pub state: State,
    /// The time of the last transition.
    pub since_ms: u64,
    pub offered_ms: u64,
    pub offer: Option<JobOffer>,
    pub placement: Option<Placement>,
    pub unit: Option<podrunner_schema::UnitId>,
    pub runner_name: Option<String>,
    pub started_ms: Option<u64>,
    pub term_ms: Option<u64>,
    pub reason: Option<ExitReason>,
    pub exit_code: Option<i32>,
    pub refusal: Option<RefusalCode>,
    pub residue: Option<podrunner_unit::Residue>,
    /// The source sent `completed` for the job.
    pub completed: bool,
}

/// The egress module as slots sees it (`podrunner-egress` behind it): one
/// listener for each unit, bound to its job, and a health.
pub trait EgressPort: Send {
    /// Serve `policy` (JSON of `broker-policy.v1`) for the unit `unit` on
    /// the socket `socket`.
    fn open(&self, unit: &str, policy: &str, socket: &std::path::Path) -> Result<(), String>;
    fn close(&self, unit: &str);
    /// False when the module ended: slots gives no capacity and ends the
    /// running units.
    fn alive(&self) -> bool;
}

/// The supervisor of the slots.
pub struct Slots {
    pub(crate) cfg: Config,
    pub(crate) clock: Arc<dyn Clock>,
    /// The time of the Unix epoch in seconds, for the age of a payload.
    pub(crate) now_unix: Box<dyn Fn() -> u64 + Send>,
    pub(crate) sources: Vec<Source>,
    pub(crate) kinds: Vec<UnitKind>,
    pub(crate) jobs: BTreeMap<(usize, JobId), Job>,
    pub(crate) suppressed: BTreeMap<JobId, u64>,
    pub(crate) journal: Journal,
    pub(crate) audit: Audit,
    pub(crate) ids: UnitIds,
    pub(crate) draining: bool,
    /// Set when a residue is above 0 or `unknown`: capacity 0, and no start
    /// until an operator clears it.
    pub(crate) degraded: Option<String>,
    /// Each report of this process, for `status` and the tests.
    pub reports: Vec<podrunner_schema::RunReport>,
    pub(crate) state_dir: std::path::PathBuf,
    pub(crate) egress: Option<Box<dyn EgressPort>>,
    /// The egress module ended: fail closed.
    pub(crate) egress_down: bool,
}

impl Slots {
    pub fn now(&self) -> u64 {
        self.clock.now_ms()
    }

    /// Give each unit a listener of the egress module.
    pub fn with_egress(mut self, egress: Box<dyn EgressPort>) -> Slots {
        self.egress = Some(egress);
        self
    }

    /// One step of each job.
    pub fn tick(&mut self) {
        let now = self.now();
        self.check_egress(now);
        self.progress(now);
        for s in 0..self.sources.len() {
            self.poll(s, now);
        }
        self.retry_waiting(now);
    }

    pub fn drain(&mut self) {
        self.draining = true;
    }

    pub fn degraded(&self) -> Option<&str> {
        self.degraded.as_deref()
    }

    /// The operator clears `degraded` after a look at the host.
    pub fn clear_degraded(&mut self) {
        self.degraded = None;
    }

    pub fn jobs(&self) -> impl Iterator<Item = &Job> {
        self.jobs.values()
    }

    pub fn audit_lines(&self) -> &[Entry] {
        &self.audit.lines
    }

    pub(crate) fn kind_views(&self) -> Vec<Kind> {
        self.kinds
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let units: Vec<&Job> = self
                    .jobs
                    .values()
                    .filter(|j| {
                        j.unit.is_some() && !j.state.is_final() && j.placement.as_ref().is_some_and(|p| p.kind == i)
                    })
                    .collect();
                let repos = units
                    .iter()
                    .filter_map(|j| {
                        j.offer.as_ref().map(|o| format!("{}/{}", o.trust_inputs.owner, o.trust_inputs.repo))
                    })
                    .collect();
                Kind {
                    capability: k.capability.clone(),
                    free: k.cfg.slots.saturating_sub(units.len() as u32),
                    running: units.len() as u32,
                    repos,
                }
            })
            .collect()
    }

    /// The offers that a source can take now.
    fn capacity(&self, s: usize) -> u32 {
        let src = &self.sources[s];
        if self.draining || self.degraded.is_some() || src.revoked || self.egress_down {
            return 0;
        }
        let free: u32 = self
            .kind_views()
            .iter()
            .filter(|k| k.capability.arch == src.cfg.arch && k.capability.boundary >= self.cfg.floor(src.cfg.tier))
            .map(|k| k.free)
            .sum();
        let mine =
            |pred: fn(State) -> bool| self.jobs.values().filter(|j| j.source == s && pred(j.state)).count() as u32;
        let held = mine(|st| !st.is_final() && st != State::Offered);
        let waiting = mine(|st| st == State::Offered);
        let limit = src.cfg.slots.map(|n| n.saturating_sub(held)).unwrap_or(u32::MAX);
        free.min(limit).saturating_sub(waiting)
    }

    fn poll(&mut self, s: usize, now: u64) {
        let capacity = self.capacity(s);
        let events = match self.sources[s].source.next(capacity, Duration::ZERO) {
            Ok(ev) => ev,
            Err(e) if e.code == SourceCode::SourceRevoked => {
                self.sources[s].revoked = true;
                return;
            }
            Err(_) => return,
        };
        for ev in events {
            match ev {
                Event::Offer { offer } => self.intake(s, offer, now),
                Event::Started { job_id, runner_name } => self.on_started(s, &job_id, &runner_name, now),
                Event::Cancel { job_id } => self.on_cancel(s, &job_id, now),
                Event::Completed { job_id, .. } => {
                    if let Some(j) = self.jobs.get_mut(&(s, job_id)) {
                        j.completed = true;
                    }
                }
            }
        }
    }

    pub(crate) fn audit(
        &mut self,
        kind: audit::Kind,
        s: usize,
        job: &JobId,
        code: Option<RefusalCode>,
        detail: Option<String>,
    ) {
        let e = Entry {
            t_ms: self.now(),
            audit: kind,
            source: self.sources[s].cfg.name.clone(),
            job_id: job.clone(),
            code,
            detail,
        };
        let _ = self.sources[s].source.audit(&e.to_map());
        self.audit.write(e);
    }

    /// Move a job to `state`, with a journal line.
    pub(crate) fn set(&mut self, key: &(usize, JobId), state: State, now: u64) {
        let Some(j) = self.jobs.get_mut(key) else { return };
        j.state = state;
        j.since_ms = now;
        let kind = j.placement.as_ref().map(|p| self.kinds[p.kind].name.clone());
        let line = Line {
            t_ms: now,
            source: self.sources[key.0].cfg.name.clone(),
            job_id: key.1.clone(),
            state,
            unit_id: j.unit.clone(),
            unit_kind: kind,
            reason: j.reason,
            code: j.refusal,
        };
        // A journal that cannot be written makes the host degraded: a
        // restart could not find the units.
        if self.journal.write(&line).is_err() {
            self.degraded = Some("the journal cannot be written".into());
        }
    }

    fn intake(&mut self, s: usize, offer: Offer, now: u64) {
        let id = offer.job_id.clone();
        if self.suppressed.get(&id).is_some_and(|until| *until > now) {
            return;
        }
        let key = (s, id.clone());
        if self.jobs.get(&key).is_some_and(|j| !j.state.is_final()) {
            return; // a redelivery
        }
        let mut fields = offer.fields;
        fields.insert("job_id".into(), Value::String(id.to_string()));
        let parsed: Option<JobOffer> = serde_json::from_value(Value::Object(fields)).ok();
        let scaleset = self.sources[s].cfg.kind == SourceKind::GithubScaleset;
        let job = Job {
            source: s,
            id: id.clone(),
            state: State::Offered,
            since_ms: now,
            offered_ms: now,
            offer: parsed.clone().filter(|o| o.check(scaleset).is_ok()),
            placement: None,
            unit: None,
            runner_name: None,
            started_ms: None,
            term_ms: None,
            reason: None,
            exit_code: None,
            refusal: None,
            residue: None,
            completed: false,
        };
        let valid = job.offer.is_some();
        self.jobs.insert(key.clone(), job);
        self.set(&key, State::Offered, now);
        self.audit(audit::Kind::Offer, s, &id, None, None);
        if !valid {
            return self.refuse(&key, RefusalCode::EventNotAllowed, now);
        }
        if self.sources[s].source.ack(&id).is_err() {
            return self.set(&key, State::Released, now);
        }
        self.try_place(&key, now);
    }

    /// Place a waiting offer, and run it on when it has a place.
    pub(crate) fn try_place(&mut self, key: &(usize, JobId), now: u64) {
        let (s, offer) = match self.jobs.get(key) {
            Some(Job { state: State::Offered, offer: Some(o), source, .. }) => (*source, o.clone()),
            _ => return,
        };
        if self.draining || self.degraded.is_some() {
            return;
        }
        let set = self.sources[s].cfg.tier;
        match placement::place(&offer, set, &self.kind_views(), &self.cfg, (self.now_unix)()) {
            Ok(p) => {
                let tier = format!("{:?}", p.tier).to_lowercase();
                self.audit(audit::Kind::Tier, s, &key.1, None, Some(tier));
                if p.payload_ignored {
                    self.audit(audit::Kind::PayloadIgnored, s, &key.1, None, None);
                }
                let unit = self.ids.next();
                let runner = format!("podrunner-{}-{unit}", self.cfg.host);
                if let Some(j) = self.jobs.get_mut(key) {
                    j.placement = Some(p);
                    j.unit = Some(unit);
                    j.runner_name = Some(runner);
                }
                self.set(key, State::Matched, now);
                self.claim(key, now);
            }
            Err(code) if code.is_wait() => {}
            Err(code) => self.refuse(key, code, now),
        }
    }

    /// A terminal refusal: withdraw the offer, report it, and suppress the
    /// job id.
    pub(crate) fn refuse(&mut self, key: &(usize, JobId), code: RefusalCode, now: u64) {
        let s = key.0;
        let _ = self.sources[s].source.withdraw(&key.1, code.as_str());
        if let Some(j) = self.jobs.get_mut(key) {
            j.refusal = Some(code);
            j.reason = Some(ExitReason::Refused);
            j.unit = None;
            j.placement = None;
        }
        self.audit(audit::Kind::Refuse, s, &key.1, Some(code), None);
        self.suppressed.insert(key.1.clone(), now + self.cfg.terminal_suppress_s * 1000);
        self.report(key);
        self.set(key, State::Refused, now);
    }

    fn retry_waiting(&mut self, now: u64) {
        let waiting: Vec<(usize, JobId)> = self
            .jobs
            .iter()
            .filter(|(_, j)| j.state == State::Offered && j.offer.is_some())
            .map(|(k, _)| k.clone())
            .collect();
        for key in waiting {
            let deadline = self.jobs[&key].offer.as_ref().map(|o| o.wait_deadline_s).unwrap_or(0) * 1000;
            if now.saturating_sub(self.jobs[&key].offered_ms) >= deadline {
                let _ = self.sources[key.0].source.withdraw(&key.1, "wait-deadline");
                if let Some(j) = self.jobs.get_mut(&key) {
                    j.reason = Some(ExitReason::Withdrawn);
                }
                self.report(&key);
                self.set(&key, State::Released, now);
            } else {
                self.try_place(&key, now);
            }
        }
    }

    /// The tier of a source.
    pub fn source_tier(&self, s: usize) -> Tier {
        self.sources[s].cfg.tier
    }
}
