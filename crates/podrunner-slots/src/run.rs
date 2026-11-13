//! The life of a unit: claim, mint, start, the `started` event, the wall
//! time, the end, destroy, report, release.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use podrunner_schema::{ExitReason, JobId, RefusalCode, Residual, RunReport, UnitFields};
use podrunner_source::SourceCode;
use podrunner_unit::{Count, Exit, Signal, StartSpec};

use crate::audit;
use crate::engine::Slots;
use crate::state::State;

type Key = (usize, JobId);

/// The exit code of the official runner when the forge refuses its version.
const RUNNER_TOO_OLD: i32 = 7;

fn residual(c: Count) -> Residual {
    match c {
        Count::Known(n) => Residual::Count(n),
        Count::Unknown => Residual::Unknown(podrunner_schema::report::Unknown::Unknown),
    }
}

/// The longest path of a unix socket that each kernel takes (the field
/// `sun_path` has 108 bytes with its end).
const SOCKET_PATH_MAX: usize = 100;

impl Slots {
    /// End the sockets of `unit` (control, egress) and remove its socket
    /// directory.
    pub(crate) fn close_sockets(&self, unit: &podrunner_schema::UnitId) {
        self.close_hooks(unit);
        if let Some(e) = &self.egress {
            e.close(unit.as_str());
        }
        let _ = std::fs::remove_dir_all(socket_dir(&self.state_dir, unit));
    }
}

pub(crate) use podrunner_unit::{CONTROL_SOCKET, EGRESS_SOCKET};

/// The socket directory of `unit`: its egress socket, its control socket
/// and the hook relay, and nothing of another unit.
pub(crate) fn socket_dir(state: &std::path::Path, unit: &podrunner_schema::UnitId) -> std::path::PathBuf {
    egress_dir(state).join(unit.as_str())
}

/// The directory of the socket directories: `STATE/egress`, or, when a
/// socket path there would be too long, a short directory of this state
/// directory under the temporary directory.
pub(crate) fn egress_dir(state: &std::path::Path) -> std::path::PathBuf {
    let dir = state.join("egress");
    if dir.as_os_str().len() + "/u-18446744073709551615/egress.sock".len() <= SOCKET_PATH_MAX {
        return dir;
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in state.as_os_str().as_encoded_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    std::env::temp_dir().join(format!("podrunner-{h:016x}"))
}

impl Slots {
    pub(crate) fn claim(&mut self, key: &Key, now: u64) {
        let runner = self.jobs[key].runner_name.clone().unwrap_or_default();
        // The intent goes to the disk first (GitHub #55): after a death of
        // the process or the host, the replay releases and reports a job
        // that the source may hold. A journal that cannot take it claims
        // nothing.
        if !self.set(key, State::Claimed, now) || self.journal.sync().is_err() {
            self.degraded = Some("the journal cannot be written".into());
            if let Some(j) = self.jobs.get_mut(key) {
                j.unit = None;
                j.placement = None;
            }
            self.set(key, State::Released, now);
            return;
        }
        match self.sources[key.0].source.claim(&key.1, &runner) {
            Ok(()) => {
                self.audit(audit::Kind::Claim, key.0, &key.1, None, None);
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
        self.jobs.get_mut(key).expect("a minted job").minted_ms = Some(now);
        let j = &self.jobs[key];
        let (p, unit) = (j.placement.clone().expect("a placed job"), j.unit.clone().expect("a unit id"));
        let kind = &self.kinds[p.kind];
        let arch = if kind.capability.arch == "aarch64" { "arm64" } else { "amd64" };
        let mut env = BTreeMap::from([
            ("PODRUNNER_UNIT".to_string(), unit.to_string()),
            // The runner exits 7 on a "version too old" answer only with the
            // string "1" here (SPEC §11.3); slots then marks the pin stale.
            ("ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE".to_string(), "1".to_string()),
        ]);
        // One socket directory for each unit: its egress socket, its
        // control socket and the relay. A driver that mounts it into the
        // unit gets it as `sockets`, and the unit sees the guest paths.
        let dir = socket_dir(&self.state_dir, &unit);
        let guest = kind.driver.socket_mount().map(str::to_string);
        let seen = |name: &str| match &guest {
            Some(g) => format!("{}/{name}", g.trim_end_matches('/')),
            None => dir.join(name).display().to_string(),
        };
        let made = std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)));
        if let Err(e) = made {
            self.audit(audit::Kind::Refuse, key.0, &key.1, None, Some(format!("the socket directory: {}", e.kind())));
            return self.give_up(key, RefusalCode::UnitStartFailed, now);
        }
        if let Some(e) = &self.egress {
            let policy = self.policy_of(key);
            let socket = dir.join(EGRESS_SOCKET);
            let opened = e.open(unit.as_str(), &policy, &socket);
            match opened {
                Err(why) => {
                    self.audit(audit::Kind::Refuse, key.0, &key.1, None, Some(format!("egress: {why}")));
                    return self.give_up(key, RefusalCode::UnitStartFailed, now);
                }
                Ok(0) => {}
                Ok(n) => self.audit(audit::Kind::EgressFlagged, key.0, &key.1, None, Some(format!("{n} rules"))),
            }
            // The way out of the unit: a socket of its job alone. No token.
            env.insert("PODRUNNER_EGRESS".into(), format!("http://unix:{}", seen(EGRESS_SOCKET)));
        }
        // The age of the payload again at the start (GitHub #30): a claim
        // can wait long enough for a pin to pass its limit.
        let now_s = (self.now_unix)();
        let pinned = |x: &crate::config::Payload| {
            x.reference == p.payload || podrunner_unit::parse_pinned(&x.reference).is_ok_and(|(_, d)| d == p.payload)
        };
        if self.cfg.pin.payload.iter().any(|x| pinned(x) && x.stale(&self.cfg.pin, now_s)) {
            self.close_sockets(&unit);
            return self.give_up(key, RefusalCode::ImageStale, now);
        }
        match self.open_hooks(p.kind, &unit, &dir, guest.as_deref()) {
            Ok(vars) => env.extend(vars),
            Err(why) => {
                self.audit(audit::Kind::Refuse, key.0, &key.1, None, Some(format!("hooks: {why}")));
                self.close_sockets(&unit);
                return self.give_up(key, RefusalCode::UnitStartFailed, now);
            }
        }
        let kind = &self.kinds[p.kind];
        let started = kind.driver.prepare(&p.payload, &format!("linux/{arch}"), false).and_then(|image| {
            kind.driver.start(&StartSpec {
                unit: unit.clone(),
                image,
                argv: kind.cfg.command.clone(),
                env,
                credential,
                // Each driver gets the directory: the microvm kind bridges
                // its egress socket into the guest (T-107).
                sockets: Some(dir.clone()),
            })
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
                self.close_sockets(&unit);
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
            self.close_sockets(u);
        }
        self.set(key, State::Destroyed, now);
        self.report(key);
        let _ = self.sources[key.0].source.release(&key.1);
        self.set(key, State::Reported, now);
    }

    /// The egress policy of a job: the policy of its offer narrowed by
    /// `default_policy` of `[egress]` (T-107), else either one, else no
    /// way out at all.
    fn policy_of(&self, key: &Key) -> String {
        let offer = self.jobs[key].offer.as_ref().map(|o| o.policy.clone()).filter(|p| !p.is_empty());
        let default = self.cfg.egress.default_policy.clone();
        let (offer, default) = (offer.map(serde_json::Value::Object), default.map(serde_json::Value::Object));
        match (offer, default) {
            (Some(o), Some(d)) => crate::policy::narrow(&d, &o).to_string(),
            (Some(p), None) | (None, Some(p)) => p.to_string(),
            (None, None) => r#"{"schema":1,"mode":"allow","allow":[]}"#.into(),
        }
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
        let unit_ran = unit.is_some();
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
            claim_to_first_step_s: j.claimed_ms.zip(j.first_step_ms).map(|(c, f)| f.saturating_sub(c) / 1000),
            job_images: j.images.clone(),
            // The use of the unit, when a unit ran (T-108).
            cpu_s: unit_ran.then(|| j.usage.report().0),
            peak_rss_mib: unit_ran.then(|| j.usage.report().1),
        };
        let value = if self.sources[key.0].cfg.operator_controlled {
            serde_json::to_value(&r)
        } else {
            serde_json::to_value(r.public())
        };
        if let Ok(serde_json::Value::Object(m)) = value {
            self.send_report(key.0, &key.1, m);
        }
        crate::forget::keep(&mut self.reports, r);
    }
}
