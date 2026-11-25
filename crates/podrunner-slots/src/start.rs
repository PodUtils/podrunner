//! The start of a unit: claim, mint, the socket directory, the egress,
//! the hooks, and `start` of the driver; `give_up` ends a job that got no
//! unit.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;

use podrunner_schema::{ExitReason, JobId, RefusalCode};
use podrunner_source::SourceCode;
use podrunner_unit::StartSpec;

use crate::audit;
use crate::engine::Slots;
use crate::run::job_file;
use crate::state::State;

type Key = (usize, JobId);

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
        crate::own::end_socket_dir(&self.state_dir, unit);
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
pub fn egress_dir(state: &std::path::Path) -> std::path::PathBuf {
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
        // T-175: the job that this unit is for, for the job-started hook of
        // the runner image.
        let mut credential = credential;
        if let Some(f) = self.jobs[key].offer.as_ref().filter(|o| o.sandbox.is_none()).and_then(job_file) {
            credential.files.push(f);
        }
        self.audit(audit::Kind::Mint, key.0, &key.1, None, None);
        self.set(key, State::Minted, now);
        let minted = self.jobs.get_mut(key).expect("a minted job");
        minted.minted_ms = Some(now);
        minted.times.marks = crate::log_tail::Marks::of(&credential);
        // The work folder of the kind (T-541, T-590): the hosted path.
        let kind_of = |p: &crate::placement::Placement| &self.kinds[p.kind];
        let folder = self.jobs[key].placement.as_ref().map(kind_of).and_then(|k| k.cfg.work_folder_of(&k.name));
        if let Some(Err(why)) = folder.map(|f| crate::work_folder::apply(&mut credential, &f)) {
            self.audit(audit::Kind::Refuse, key.0, &key.1, None, Some(format!("work_folder: {why}")));
            return self.give_up(key, RefusalCode::UnitStartFailed, now);
        }
        let j = &self.jobs[key];
        let (p, unit) = (j.placement.clone().expect("a placed job"), j.unit.clone().expect("a unit id"));
        let kind = &self.kinds[p.kind];
        let arch = if kind.capability.arch == "aarch64" { "arm64" } else { "amd64" };
        let mut env = BTreeMap::from([
            ("PODRUNNER_UNIT".to_string(), unit.to_string()),
            // The runner exits 7 on a "version too old" answer only with the
            // string "1" here (SPEC §11.3); slots then marks the pin stale.
            ("ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE".to_string(), "1".to_string()),
            // With "true" the listener copies the stdout of its worker to
            // the log of the unit, and a trace line can hold text of a step;
            // `log_tail::job_result` reads that log (T-568).
            ("ACTIONS_RUNNER_PRINT_LOG_TO_STDOUT".to_string(), "false".to_string()),
        ]);
        crate::sandbox::env(j.offer.as_ref(), &mut env);
        // One socket directory for each unit: its egress socket, its
        // control socket and the relay. A driver that mounts it into the
        // unit gets it as `sockets`, and the unit sees the guest paths.
        let dir = socket_dir(&self.state_dir, &unit);
        let guest = kind.driver.socket_mount().map(str::to_string);
        let cred_dir = kind.driver.credential_dir(&unit);
        let seen = |name: &str| match &guest {
            Some(g) => format!("{}/{name}", g.trim_end_matches('/')),
            None => dir.join(name).display().to_string(),
        };
        let made = crate::own::own_root(&egress_dir(&self.state_dir))
            .and_then(|()| std::fs::create_dir_all(&dir))
            .and_then(|()| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)));
        if let Err(e) = made {
            self.audit(audit::Kind::Refuse, key.0, &key.1, None, Some(format!("the socket directory: {}", e.kind())));
            return self.give_up(key, RefusalCode::UnitStartFailed, now);
        }
        if self.egress.is_some()
            && self.cfg.egress.default_policy.is_none()
            && self.jobs[key].offer.as_ref().is_some_and(|o| !o.policy.is_empty())
        {
            let why = "egress: the policy of the offer opens nothing with no [egress] default_policy (T-410)";
            self.audit(audit::Kind::Refuse, key.0, &key.1, None, Some(why.into()));
        }
        let mut unit_ca: Option<(String, Vec<String>)> = None;
        if let Some(e) = &self.egress {
            let offer = self.jobs[key].offer.as_ref().map(|o| &o.policy);
            let policy = crate::policy::of_job(offer, self.cfg.egress.default_policy.as_ref());
            let socket = dir.join(EGRESS_SOCKET);
            let store = &self.cfg.egress.secrets;
            let of = (unit.as_str(), self.sources[key.0].cfg.name.as_str());
            match crate::secrets::open(e.as_ref(), of, &policy, store, &socket, cred_dir.as_deref()) {
                Err(why) => {
                    self.audit(audit::Kind::Refuse, key.0, &key.1, None, Some(format!("egress: {why}")));
                    return self.give_up(key, RefusalCode::UnitStartFailed, now);
                }
                Ok(o) => {
                    if o.flagged > 0 {
                        let n = o.flagged;
                        self.audit(audit::Kind::EgressFlagged, key.0, &key.1, None, Some(format!("{n} rules")));
                    }
                    // The unit trusts the CA of its broker (T-563), and each
                    // container of its job too (T-578).
                    if let Some(t) = o.ca {
                        unit_ca = Some(t.bundle);
                        credential.files.extend(t.files);
                        env.extend(t.env);
                    }
                }
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
        let ca = unit_ca.as_ref().map(|(pem, vars)| crate::hooks::ContainerCa {
            file: crate::secrets::CA_FILE,
            pem: pem.as_str(),
            vars: vars.as_slice(),
        });
        match self.open_hooks(p.kind, &unit, &dir, guest.as_deref(), ca) {
            Ok(vars) => env.extend(vars),
            Err(why) => {
                self.audit(audit::Kind::Refuse, key.0, &key.1, None, Some(format!("hooks: {why}")));
                self.close_sockets(&unit);
                return self.give_up(key, RefusalCode::UnitStartFailed, now);
            }
        }
        // The limits of the offer: the driver applies the lesser of each and
        // the limit of its kind (T-198, the owner, 2026-11-15).
        let job_limits = self.jobs[key]
            .offer
            .as_ref()
            .map(|o| podrunner_unit::limits::Limits::of_offer(&o.limits))
            .unwrap_or_default();
        // The cache of the job, kept apart by trust (T-480).
        let mounts = self.cache_start(key, &mut env);
        let kind = &self.kinds[p.kind];
        let started = kind.driver.prepare(&p.payload, &format!("linux/{arch}"), false).and_then(|image| {
            kind.driver.start(&StartSpec {
                unit: unit.clone(),
                image,
                argv: crate::sandbox::argv(self.jobs[key].offer.as_ref(), &kind.cfg.command),
                env,
                credential,
                // Each driver gets the directory: the microvm kind bridges
                // its egress socket into the guest (T-107).
                sockets: Some(dir.clone()),
                mounts,
                limits: job_limits,
            })
        });
        let driver = kind.driver.clone();
        match started {
            Ok(()) => {
                if let Some(j) = self.jobs.get_mut(key) {
                    j.started_ms = Some(now);
                }
                self.set(key, State::Started, now);
            }
            Err(e) => {
                // The cause for the operator (T-176): the report has the
                // code only. `give_up` releases the minted runner.
                let why = format!("start: {:?}: {}", e.code, e.detail);
                eprintln!("podrunner: {unit}: {why}");
                self.audit(audit::Kind::Refuse, key.0, &key.1, None, Some(why));
                let _ = driver.destroy(&unit);
                self.close_sockets(&unit);
                self.give_up(key, RefusalCode::UnitStartFailed, now);
            }
        }
    }

    /// End a claimed job with no unit: report the refusal, then release
    /// it, so that the source knows the refusal at the release (T-200).
    pub(crate) fn give_up(&mut self, key: &Key, code: RefusalCode, now: u64) {
        if let Some(j) = self.jobs.get_mut(key) {
            j.refusal = Some(code);
            j.reason = Some(ExitReason::Refused);
            j.unit = None;
            j.placement = None;
        }
        self.audit(audit::Kind::Refuse, key.0, &key.1, Some(code), None);
        self.report(key);
        let _ = self.sources[key.0].source.release(&key.1);
        self.set(key, State::Failed, now);
    }
}
