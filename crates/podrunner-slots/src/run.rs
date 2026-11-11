//! The life of a unit: claim, mint, start, the `started` event, the wall
//! time, the end, destroy, report, release.

use std::collections::BTreeMap;
use std::time::Duration;

use podrunner_schema::{ExitReason, JobId, RefusalCode, Residual, RunReport, UnitFields};
use podrunner_source::SourceCode;
use podrunner_unit::{Count, Exit, Signal, StartSpec};

use crate::audit;
use crate::engine::Slots;
use crate::state::State;

type Key = (usize, JobId);

fn residual(c: Count) -> Residual {
    match c {
        Count::Known(n) => Residual::Count(n),
        Count::Unknown => Residual::Unknown(podrunner_schema::report::Unknown::Unknown),
    }
}

impl Slots {
    pub(crate) fn claim(&mut self, key: &Key, now: u64) {
        let runner = self.jobs[key].runner_name.clone().unwrap_or_default();
        match self.sources[key.0].source.claim(&key.1, &runner) {
            Ok(()) => {
                self.audit(audit::Kind::Claim, key.0, &key.1, None, None);
                self.set(key, State::Claimed, now);
                self.mint(key, now);
            }
            // Another runner has the job, or the source gave it up.
            Err(_) => {
                if let Some(j) = self.jobs.get_mut(key) {
                    j.unit = None;
                    j.placement = None;
                }
                self.set(key, State::Released, now);
            }
        }
    }

    /// Mint the credential and start the unit. A `credential-unavailable`
    /// mint is tried again on the next tick, up to `mint_s`.
    pub(crate) fn mint(&mut self, key: &Key, now: u64) {
        let runner = self.jobs[key].runner_name.clone().unwrap_or_default();
        let credential = match self.sources[key.0].source.mint(&key.1, &runner) {
            Ok(c) => c,
            Err(e) if e.code == SourceCode::CredentialUnavailable => {
                if now.saturating_sub(self.jobs[key].since_ms) >= self.cfg.deadlines.mint_s * 1000 {
                    self.give_up(key, RefusalCode::CredentialUnavailable, now);
                }
                return;
            }
            Err(e) => {
                let code = if e.code == SourceCode::SourceRevoked {
                    self.sources[key.0].revoked = true;
                    RefusalCode::SourceRevoked
                } else {
                    RefusalCode::CredentialUnavailable
                };
                return self.give_up(key, code, now);
            }
        };
        self.audit(audit::Kind::Mint, key.0, &key.1, None, None);
        self.set(key, State::Minted, now);
        let j = &self.jobs[key];
        let (p, unit) = (j.placement.clone().expect("a placed job"), j.unit.clone().expect("a unit id"));
        let kind = &self.kinds[p.kind];
        let arch = if kind.capability.arch == "aarch64" { "arm64" } else { "amd64" };
        let started = kind.driver.prepare(&p.payload, &format!("linux/{arch}"), false).and_then(|image| {
            let env = BTreeMap::from([("PODRUNNER_UNIT".to_string(), unit.to_string())]);
            kind.driver.start(&StartSpec { unit: unit.clone(), image, argv: kind.cfg.command.clone(), env, credential })
        });
        match started {
            Ok(()) => {
                if let Some(j) = self.jobs.get_mut(key) {
                    j.started_ms = Some(now);
                }
                self.set(key, State::Started, now);
            }
            Err(_) => {
                let _ = kind.driver.destroy(&unit);
                self.give_up(key, RefusalCode::UnitStartFailed, now);
            }
        }
    }

    /// End a claimed job with no unit: release it and report the refusal.
    fn give_up(&mut self, key: &Key, code: RefusalCode, now: u64) {
        let _ = self.sources[key.0].source.release(&key.1);
        if let Some(j) = self.jobs.get_mut(key) {
            j.refusal = Some(code);
            j.reason = Some(ExitReason::Refused);
            j.unit = None;
            j.placement = None;
        }
        self.audit(audit::Kind::Refuse, key.0, &key.1, Some(code), None);
        self.report(key);
        self.set(key, State::Failed, now);
    }

    pub(crate) fn on_started(&mut self, s: usize, job: &JobId, runner: &str, now: u64) {
        let key = (s, job.clone());
        let Some(j) = self.jobs.get(&key) else { return };
        if j.state != State::Started {
            return;
        }
        if j.runner_name.as_deref() == Some(runner) {
            return self.set(&key, State::Running, now);
        }
        // The forge gave the job to another runner: this unit must not run.
        self.audit(audit::Kind::JobMismatch, s, job, None, None);
        self.end(&key, ExitReason::JobMismatch, Signal::Kill, now);
    }

    pub(crate) fn on_cancel(&mut self, s: usize, job: &JobId, now: u64) {
        let key = (s, job.clone());
        match self.jobs.get(&key).map(|j| j.state) {
            Some(State::Offered) => {
                let _ = self.sources[s].source.withdraw(job, "cancelled");
                if let Some(j) = self.jobs.get_mut(&key) {
                    j.reason = Some(ExitReason::Cancelled);
                }
                self.report(&key);
                self.set(&key, State::Released, now);
            }
            Some(State::Started | State::Running) => self.end(&key, ExitReason::Cancelled, Signal::Term, now),
            _ => {}
        }
    }

    /// Signal the unit and record the reason; the next ticks wait for the
    /// end, then destroy.
    fn end(&mut self, key: &Key, reason: ExitReason, signal: Signal, now: u64) {
        let (unit, kind) = {
            let j = &self.jobs[key];
            (j.unit.clone(), j.placement.as_ref().map(|p| p.kind))
        };
        if let (Some(u), Some(k)) = (unit, kind) {
            let _ = self.kinds[k].driver.signal(&u, signal);
        }
        if let Some(j) = self.jobs.get_mut(key) {
            j.reason.get_or_insert(reason);
            j.term_ms.get_or_insert(now);
        }
    }

    /// Move each unit on: an exit, a deadline, the end.
    pub(crate) fn progress(&mut self, now: u64) {
        let keys: Vec<Key> = self.jobs.iter().filter(|(_, j)| !j.state.is_final()).map(|(k, _)| k.clone()).collect();
        for key in keys {
            let j = self.jobs[&key].clone();
            match j.state {
                State::Claimed => self.mint(&key, now),
                State::Started | State::Running => {
                    let (Some(u), Some(p)) = (&j.unit, &j.placement) else { continue };
                    let d = &self.cfg.deadlines;
                    if let Ok(Some(exit)) = self.kinds[p.kind].driver.wait(u, Duration::ZERO) {
                        let code = match exit {
                            Exit::Code(c) => Some(c),
                            Exit::Signal(_) => None,
                        };
                        let reason = if code == Some(0) { ExitReason::Success } else { ExitReason::Failure };
                        if let Some(job) = self.jobs.get_mut(&key) {
                            job.exit_code = code;
                            job.reason.get_or_insert(reason);
                        }
                        self.set(&key, State::Ended, now);
                        continue;
                    }
                    let wall_s = j
                        .offer
                        .as_ref()
                        .and_then(|o| o.limits.get("wall").copied())
                        .unwrap_or(d.running_s)
                        .min(d.running_s);
                    let elapsed = now.saturating_sub(j.started_ms.unwrap_or(now));
                    if j.state == State::Started
                        && j.term_ms.is_none()
                        && now.saturating_sub(j.since_ms) >= d.start_s * 1000
                    {
                        self.end(&key, ExitReason::UnitFailed, Signal::Kill, now);
                    } else if j.term_ms.is_none() && elapsed >= wall_s * 1000 {
                        self.end(&key, ExitReason::Timeout, Signal::Term, now);
                    } else if j.term_ms.is_some_and(|t| now.saturating_sub(t) >= d.grace_s * 1000) {
                        let _ = self.kinds[p.kind].driver.signal(u, Signal::Kill);
                        self.set(&key, State::Ended, now);
                    }
                }
                State::Ended => self.finish(&key, now),
                _ => {}
            }
        }
    }

    /// Destroy the unit, report, release.
    fn finish(&mut self, key: &Key, now: u64) {
        let j = self.jobs[key].clone();
        if let (Some(u), Some(p)) = (&j.unit, &j.placement) {
            let residue = self.kinds[p.kind]
                .driver
                .destroy(u)
                .unwrap_or(podrunner_unit::Residue { processes: Count::Unknown, files: Count::Unknown });
            if !residue.is_clean() {
                self.degraded =
                    Some(format!("{u} left {:?} processes and {:?} files", residue.processes, residue.files));
            }
            if let Some(job) = self.jobs.get_mut(key) {
                job.residue = Some(residue);
            }
            self.audit(audit::Kind::Destroy, key.0, &key.1, None, None);
        }
        self.set(key, State::Destroyed, now);
        self.report(key);
        let _ = self.sources[key.0].source.release(&key.1);
        self.set(key, State::Reported, now);
    }

    /// Build the report of a job, keep it, and send it to its source: the
    /// full report to a source that the operator controls, else the
    /// public one.
    pub(crate) fn report(&mut self, key: &Key) {
        let j = &self.jobs[key];
        let unit = match (&j.unit, &j.placement, j.residue) {
            (Some(u), Some(p), Some(r)) => {
                let k = &self.kinds[p.kind];
                Some(UnitFields {
                    unit_id: u.clone(),
                    unit_kind: k.name.clone(),
                    boundary: k.capability.boundary,
                    principal: k.capability.principal,
                    egress_hold: k.capability.egress_hold,
                    payload: podrunner_unit::parse_pinned(&p.payload).map(|(_, d)| d.to_string()).unwrap_or_default(),
                    residual_processes: residual(r.processes),
                    residual_files: residual(r.files),
                })
            }
            _ => None,
        };
        let r = RunReport {
            schema: podrunner_schema::SCHEMA,
            job_id: key.1.clone(),
            source: self.sources[key.0].cfg.name.clone(),
            tier: j.placement.as_ref().map(|p| p.tier),
            host: self.cfg.host.clone(),
            exit_reason: j.reason.unwrap_or(ExitReason::Lost),
            exit_code: j.exit_code,
            wall_s: self.now().saturating_sub(j.offered_ms) / 1000,
            refusal_code: j.refusal,
            unit,
        };
        let value = if self.sources[key.0].cfg.operator_controlled {
            serde_json::to_value(&r)
        } else {
            serde_json::to_value(r.public())
        };
        if let Ok(serde_json::Value::Object(m)) = value {
            let _ = self.sources[key.0].source.report(&key.1, &m);
        }
        self.reports.push(r);
    }
}
