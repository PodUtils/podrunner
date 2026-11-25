//! The life of a unit: claim, mint, start, the `started` event, the wall
//! time, the end, destroy, report, release.

use std::time::Duration;

use podrunner_schema::{ExitReason, JobId};
use podrunner_unit::{Count, Exit, Signal};

use crate::audit;
use crate::engine::Slots;
use crate::state::State;

type Key = (usize, JobId);

pub(crate) use crate::start::CONTROL_SOCKET;
pub use crate::start::egress_dir;

/// The exit code of the official runner when the forge refuses its version.
const RUNNER_TOO_OLD: i32 = 7;

impl Slots {
    pub(crate) fn on_started(&mut self, s: usize, job: &JobId, runner: &str, now: u64) {
        let key = (s, job.clone());
        // A runner of this host that runs a job other than its claim: the
        // unit must not go on (the design test 10 of the source contract).
        let mine = self
            .jobs
            .iter()
            .find(|(k, j)| k.0 == s && k.1 != *job && j.runner_name.as_deref() == Some(runner) && !j.state.is_final());
        if let Some((k, _)) = mine {
            let k = k.clone();
            self.audit(audit::Kind::JobMismatch, s, &k.1, None, Some("started names another job".into()));
            self.end(&k, ExitReason::JobMismatch, Signal::Kill, now);
            return;
        }
        let Some(j) = self.jobs.get(&key) else { return };
        if j.state != State::Started {
            return;
        }
        if j.runner_name.as_deref() == Some(runner) {
            self.set(&key, State::Running, now);
            return;
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
    pub(crate) fn end(&mut self, key: &Key, reason: ExitReason, signal: Signal, now: u64) {
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
                        // Exit code 7 of the runner: its version is too old
                        // for the forge. No new job starts until the
                        // operator pins a new payload (GitHub #30).
                        if code == Some(RUNNER_TOO_OLD) {
                            self.degraded = Some(format!("the runner of {} is too old: pin a new payload", p.payload));
                        }
                        if let Some(job) = self.jobs.get_mut(&key) {
                            job.exit_code = code;
                            job.reason.get_or_insert(reason);
                        }
                        self.set(&key, State::Ended, now);
                        continue;
                    }
                    let wall_s = crate::sandbox::wall_s(
                        j.offer.as_ref(),
                        j.offer
                            .as_ref()
                            .and_then(|o| o.limits.get("wall").copied())
                            .unwrap_or(d.running_s)
                            .min(d.running_s),
                    );
                    let elapsed = now.saturating_sub(j.started_ms.unwrap_or(now));
                    if j.state == State::Started
                        && j.term_ms.is_none()
                        && now.saturating_sub(j.since_ms) >= d.start_s * 1000
                    {
                        // The deadline is the cause (T-197): a slow host (an
                        // emulated one) needs a larger `[deadlines] start_s`.
                        let why = format!(
                            "the runner did not take the job in [deadlines] start_s ({} s): a slow host needs a larger start_s",
                            d.start_s
                        );
                        eprintln!("podrunner: {u}: {why}");
                        self.audit(audit::Kind::Refuse, key.0, &key.1, None, Some(why));
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
        self.read_hooks(key, now);
        let j = self.jobs[key].clone();
        if let (Some(u), Some(p)) = (&j.unit, &j.placement) {
            // The exit code of the runner does not tell each end: its log
            // does (T-497, T-552, T-567, T-568).
            self.read_unit_log(key, u, p.kind);
            // A kind that measures its units itself (T-483): before the
            // destroy, while the instance is there.
            if let Some(use_of) = self.kinds[p.kind].driver.usage(u)
                && let Some(job) = self.jobs.get_mut(key)
            {
                job.usage.of_kind(use_of);
            }
            // A job that failed after a kill of the OOM killer ends with
            // `limit` (T-515, githome's G-12): also a failure that the log
            // of the runner told.
            if self.kinds[p.kind].driver.oom_kills(u).is_some_and(|n| n > 0)
                && matches!(self.jobs[key].reason, None | Some(ExitReason::Failure | ExitReason::UnitFailed))
            {
                if let Some(job) = self.jobs.get_mut(key) {
                    job.reason = Some(ExitReason::Limit);
                }
                self.audit(audit::Kind::Limit, key.0, &key.1, None, Some("memory".into()));
            }
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
            self.close_sockets(u);
            // The thread of the caches measures a `rw` cache; the report
            // waits for its bytes (`cache_tick`).
            if self.cache_end(key) {
                self.set(key, State::Destroyed, now);
                return;
            }
        }
        self.set(key, State::Destroyed, now);
        self.reported(key, now);
    }

    /// Report a destroyed job, and release it.
    pub(crate) fn reported(&mut self, key: &Key, now: u64) {
        self.report(key);
        let _ = self.sources[key.0].source.release(&key.1);
        self.set(key, State::Reported, now);
    }

    /// When the egress module ended: no capacity, and each running unit
    /// ends (fail closed).
    pub(crate) fn check_egress(&mut self, now: u64) {
        if self.egress_down || self.egress.as_ref().is_none_or(|e| e.alive()) {
            return;
        }
        self.egress_down = true;
        let running: Vec<Key> = self
            .jobs
            .iter()
            .filter(|(_, j)| matches!(j.state, State::Started | State::Running))
            .map(|(k, _)| k.clone())
            .collect();
        for key in running {
            self.end(&key, ExitReason::UnitFailed, Signal::Kill, now);
        }
    }
}

/// The name of the file of the placed job in the runner root (T-175).
pub const JOB_FILE: &str = ".podrunner-job";

/// `OWNER/REPO RUN_ID` of the offer, for the job-started hook of the runner
/// image (`images/runner/podrunner-job-started.sh`). An offer with no run
/// gives run 0, which no job matches: the hook then asks serve (T-493). An
/// offer with no repository (the local mode) gives none.
pub(crate) fn job_file(offer: &podrunner_schema::JobOffer) -> Option<podrunner_schema::CredentialFile> {
    let t = &offer.trust_inputs;
    if t.owner.is_empty() || t.repo.is_empty() {
        return None;
    }
    let text = format!("{}/{} {}\n", t.owner, t.repo, t.run_id);
    Some(podrunner_schema::CredentialFile { name: JOB_FILE.into(), content: podrunner_schema::Secret::new(&text) })
}
