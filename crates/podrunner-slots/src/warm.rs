//! Warm units (T-487): a pool of booted units with no job, for each kind
//! and each label row of `[unit.KIND] warm`. A warm unit holds no
//! credential, no job variable, no job argv, and its egress socket leads
//! nowhere until a job takes it (`warm_take.rs`). The journal keeps each
//! one under the source `@warm`, so a restart destroys it.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;

use podrunner_schema::{Capability, JobId, RefusalCode, UnitId};
use podrunner_unit::Signal;
use podrunner_unit::limits::Limits;

use crate::audit;
use crate::engine::Slots;
use crate::state::{Line, State};

/// The source of the journal and audit lines of the warm units.
pub const WARM_SOURCE: &str = "@warm";

/// The default life of a warm unit (`warm_max_age_s`).
const MAX_AGE_S: u64 = 3600;

/// The default memory that warm units leave free (`warm_reserve`).
const RESERVE: u64 = 1 << 30;

/// The wait of a row after its first failure; it doubles up to `PAUSE_MS`.
const BACKOFF_MS: u64 = 60_000;
const PAUSE_MS: u64 = 3_600_000;

/// One warm unit.
#[derive(Debug, Clone)]
pub(crate) struct Warm {
    pub kind: usize,
    pub capability: Capability,
    /// The label of its row (`*`: no label).
    pub row: String,
    /// The pinned reference that it booted.
    pub payload: String,
    pub made_ms: u64,
    pub ready_ms: Option<u64>,
    /// The job that placement gave it; then no end but `failed` and
    /// `boot-timeout` applies (the review of T-487, P2-1).
    pub reserved: Option<(usize, JobId)>,
    /// `applied_limits` of no offer: a job fits only with the same.
    pub limits: Limits,
}

/// Why a warm unit ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    Age,
    Pin,
    Stale,
    Capability,
    Pressure,
    Evict,
    Drain,
    Failed,
    BootTimeout,
    Config,
    /// The job of a reserved unit ended before the take: the source had
    /// its claim, so the unit never goes back (P2-2).
    JobEnded,
}

impl End {
    fn word(self) -> &'static str {
        match self {
            End::Age => "age",
            End::Pin => "pin",
            End::Stale => "stale",
            End::Capability => "capability",
            End::Pressure => "pressure",
            End::Evict => "evict",
            End::Drain => "drain",
            End::Failed => "failed",
            End::BootTimeout => "boot-timeout",
            End::Config => "config",
            End::JobEnded => "job-ended",
        }
    }

    /// An end that the refill backs off from (P2-6).
    fn is_failure(self) -> bool {
        matches!(self, End::Failed | End::BootTimeout | End::Pin | End::Stale)
    }
}

/// The pool of the host.
pub(crate) struct Pool {
    pub units: BTreeMap<UnitId, Warm>,
    /// The failures in a row of a row, the time of its next refill, and
    /// the payload that the row resolved to then: a new pin ends the wait.
    pub backoff: BTreeMap<(usize, String), (u32, u64, Option<String>)>,
    /// The memory that the host has free now (bytes), when it can tell.
    pub mem_probe: Box<dyn Fn() -> Option<u64> + Send>,
}

impl Default for Pool {
    fn default() -> Pool {
        Pool { units: BTreeMap::new(), backoff: BTreeMap::new(), mem_probe: Box::new(available_memory) }
    }
}

/// The lesser of `MemAvailable` and the room of the cgroup of this
/// process (`memory.max - memory.current`), in bytes.
fn available_memory() -> Option<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kib = meminfo.lines().find_map(|l| l.strip_prefix("MemAvailable:"))?.trim().trim_end_matches("kB").trim();
    let host = kib.parse::<u64>().ok()?.saturating_mul(1024);
    let cgroup = std::fs::read_to_string("/proc/self/cgroup").ok().and_then(|c| {
        let path = c.lines().find_map(|l| l.strip_prefix("0::"))?.to_string();
        let dir = std::path::Path::new("/sys/fs/cgroup").join(path.trim_start_matches('/'));
        let max = std::fs::read_to_string(dir.join("memory.max")).ok()?.trim().parse::<u64>().ok()?;
        let now = std::fs::read_to_string(dir.join("memory.current")).ok()?.trim().parse::<u64>().ok()?;
        Some(max.saturating_sub(now))
    });
    Some(cgroup.map_or(host, |c| c.min(host)))
}

impl Slots {
    /// The warm units of the host, booting or ready, reserved or free.
    pub fn warm_units(&self) -> usize {
        self.warm.units.len()
    }

    /// The free (not reserved) warm units of the kind `kind`.
    pub(crate) fn warm_free(&self, kind: usize) -> u32 {
        self.warm.units.values().filter(|w| w.kind == kind && w.reserved.is_none()).count() as u32
    }

    /// One step of the pool: watch each unit, end the ones that must go,
    /// and make at most one unit for each kind.
    pub(crate) fn warm_tick(&mut self, now: u64) {
        self.warm_watch(now);
        self.warm_retire(now);
        self.warm_refill(now);
    }

    /// The ready mark, the end of the top process, and the time to boot.
    fn warm_watch(&mut self, now: u64) {
        let units: Vec<(UnitId, Warm)> = self.warm.units.iter().map(|(u, w)| (u.clone(), w.clone())).collect();
        for (unit, w) in units {
            let d = self.kinds[w.kind].driver.clone();
            if matches!(d.wait(&unit, std::time::Duration::ZERO), Ok(Some(_)) | Err(_)) {
                self.end_warm(&unit, End::Failed, now);
                continue;
            }
            if w.ready_ms.is_some() {
                continue;
            }
            if d.warm_ready(&unit).unwrap_or(false) {
                if let Some(x) = self.warm.units.get_mut(&unit) {
                    x.ready_ms = Some(now);
                }
                self.warm.backoff.remove(&(w.kind, w.row.clone()));
                self.warm_line(&unit, w.kind, State::Started, now);
                let boot = now.saturating_sub(w.made_ms).div_ceil(1000);
                self.audit_host(&unit, format!("ready boot_s={boot}"));
            } else if now.saturating_sub(w.made_ms) > self.cfg.deadlines.start_s.saturating_mul(1000) {
                self.end_warm(&unit, End::BootTimeout, now);
            }
        }
    }

    /// End each free warm unit that must go: a drain, a smaller row, its
    /// age, a new or a stale pin, a change of the record of its kind, or
    /// memory pressure. A reserved unit stays (P2-1).
    fn warm_retire(&mut self, now: u64) {
        if self.warm.units.is_empty() {
            return;
        }
        let now_s = (self.now_unix)();
        let mut ends: Vec<(UnitId, End)> = Vec::new();
        let mut per_row: BTreeMap<(usize, String), u32> = BTreeMap::new();
        for (unit, w) in self.warm.units.iter().filter(|(_, w)| w.reserved.is_none()) {
            let k = &self.kinds[w.kind];
            let count = k.cfg.warm_rows().into_iter().find(|(l, _)| *l == w.row).map_or(0, |(_, n)| n);
            let seen = per_row.entry((w.kind, w.row.clone())).or_default();
            *seen += 1;
            let age_s = k.cfg.warm_max_age_s.unwrap_or(MAX_AGE_S);
            let end = if self.draining {
                Some(End::Drain)
            } else if *seen > count {
                Some(End::Config)
            } else if now.saturating_sub(w.made_ms) > age_s.saturating_mul(1000) {
                Some(End::Age)
            } else if w.capability != k.capability {
                Some(End::Capability)
            } else {
                match self.warm_payload(w.kind, &w.row, now_s) {
                    Err(RefusalCode::ImageStale) => Some(End::Stale),
                    Err(_) => Some(End::Pin),
                    Ok(p) if p != w.payload => Some(End::Pin),
                    Ok(_) => None,
                }
            };
            if let Some(e) = end {
                ends.push((unit.clone(), e));
            }
        }
        // Under the reserve, the newest free unit ends; one each tick.
        let short = (self.warm.mem_probe)().is_some_and(|a| a < self.warm_reserve());
        if short && ends.is_empty() {
            let newest = self.warm.units.iter().filter(|(_, w)| w.reserved.is_none()).max_by_key(|(_, w)| w.made_ms);
            if let Some((u, _)) = newest {
                ends.push((u.clone(), End::Pressure));
            }
        }
        for (unit, end) in ends {
            self.end_warm(&unit, end, now);
        }
    }

    pub(crate) fn warm_reserve(&self) -> u64 {
        let r = self.cfg.warm_reserve.as_deref().map(podrunner_unit::limits::size_bytes);
        r.and_then(Result::ok).unwrap_or(RESERVE)
    }

    /// The pinned reference that a warm unit of the row gets now: the rule
    /// of placement for a job with the label of the row (no label for
    /// `*`), with the init of the kind (P2-6).
    pub(crate) fn warm_payload(&self, kind: usize, row: &str, now_s: u64) -> Result<String, RefusalCode> {
        let k = &self.kinds[kind];
        let labels: Vec<String> = if row == crate::unit_config::ANY_LABEL { Vec::new() } else { vec![row.to_string()] };
        let p = crate::placement::pick_pinned(&self.cfg, &k.capability.arch, &labels, now_s)?;
        if p.init != k.cfg.init {
            return Err(RefusalCode::PayloadNotPinned);
        }
        Ok(p.reference.clone())
    }

    /// Make one warm unit for each kind that needs one, inside the slots,
    /// `max_units`, the memory reserve and the backoff of its row.
    fn warm_refill(&mut self, now: u64) {
        if self.draining || self.degraded.is_some() || self.egress_down {
            return;
        }
        let views = self.kind_views();
        let running_total: u32 = views.iter().map(|k| k.running).sum();
        let now_s = (self.now_unix)();
        for (kind, view) in views.iter().enumerate() {
            let k = &self.kinds[kind];
            let rows = self.warm_pending_rows(kind);
            if rows.is_empty() {
                continue;
            }
            // Counted again for each kind: a unit made for an earlier kind
            // in this tick holds a unit of `max_units` too.
            let free_total = self.warm.units.values().filter(|w| w.reserved.is_none()).count() as u32;
            let in_slots = view.running + self.warm_free(kind) < k.cfg.slots;
            let in_host = self.cfg.max_units.is_none_or(|m| running_total + free_total < m);
            let unit_mem = crate::limits::limits_of(&k.cfg).ok().and_then(|l| l.memory_bytes).unwrap_or(0);
            let room = (self.warm.mem_probe)().is_none_or(|a| a >= unit_mem.saturating_add(self.warm_reserve()));
            if !in_slots || !in_host || !room {
                continue;
            }
            for row in rows {
                let key = (kind, row.clone());
                if let Some((_, at, pin)) = self.warm.backoff.get(&key) {
                    if *pin != self.warm_payload(kind, &row, now_s).ok() {
                        self.warm.backoff.remove(&key);
                    } else if now < *at {
                        continue;
                    }
                }
                self.warm_one(kind, row, now);
                break;
            }
        }
    }

    /// Rows whose targets are not yet met.
    pub(crate) fn warm_pending_rows(&self, kind: usize) -> Vec<String> {
        self.kinds[kind]
            .cfg
            .warm_rows()
            .into_iter()
            .filter_map(|(row, count)| {
                let have = self.warm.units.values().filter(|w| w.kind == kind && w.row == row).count();
                (have < count as usize).then_some(row)
            })
            .collect()
    }

    /// Boot one warm unit of `row` on the kind `kind`.
    fn warm_one(&mut self, kind: usize, row: String, now: u64) {
        let payload = match self.warm_payload(kind, &row, (self.now_unix)()) {
            Ok(p) => p,
            Err(code) => {
                // No unit: the row backs off as for a failed boot.
                self.warm_backoff(kind, &row, now);
                let name = self.kinds[kind].name.clone();
                eprintln!("podrunner: [unit.{name}] warm {row:?}: no payload: {}", code.as_str());
                return;
            }
        };
        let unit = self.ids.next();
        let k = &self.kinds[kind];
        let limits = k.driver.applied_limits(&Limits::default());
        let w = Warm {
            kind,
            capability: k.capability.clone(),
            row: row.clone(),
            payload: payload.clone(),
            made_ms: now,
            ready_ms: None,
            reserved: None,
            limits,
        };
        self.warm.units.insert(unit.clone(), w);
        if !self.warm_line(&unit, kind, State::Matched, now) {
            self.warm.units.remove(&unit);
            return;
        }
        let digest = podrunner_unit::parse_pinned(&payload).map(|(_, d)| d.to_string()).unwrap_or_default();
        let name = self.kinds[kind].name.clone();
        self.audit_host(&unit, format!("start kind={name} row={row} payload={digest}"));
        if let Err(why) = self.warm_boot(kind, &unit, &payload) {
            eprintln!("podrunner: {unit}: warm: {why}");
            self.audit_host(&unit, format!("start failed: {why}"));
            self.end_warm(&unit, End::Failed, now);
        }
    }

    /// The socket directory, the hold of its egress, and `warm` of the
    /// driver.
    fn warm_boot(&mut self, kind: usize, unit: &UnitId, payload: &str) -> Result<(), String> {
        let dir = crate::start::socket_dir(&self.state_dir, unit);
        crate::own::own_root(&crate::run::egress_dir(&self.state_dir))
            .and_then(|()| std::fs::create_dir_all(&dir))
            .and_then(|()| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)))
            .map_err(|e| format!("the socket directory: {}", e.kind()))?;
        if let Some(e) = &self.egress {
            e.hold(unit.as_str(), &dir.join(crate::start::EGRESS_SOCKET)).map_err(|w| format!("egress: {w}"))?;
        }
        let k = &self.kinds[kind];
        let arch = if k.capability.arch == "aarch64" { "arm64" } else { "amd64" };
        let image = k.driver.prepare(payload, &format!("linux/{arch}"), false).map_err(|e| e.to_string())?;
        let env = BTreeMap::from([("PODRUNNER_UNIT".to_string(), unit.to_string())]);
        let spec = podrunner_unit::warm::WarmSpec { unit: unit.clone(), image, sockets: Some(dir), env };
        k.driver.warm(&spec).map_err(|e| e.to_string())
    }

    /// End the oldest free warm unit of `kind` when one more unit would
    /// pass its slots (a guest self-test).
    pub(crate) fn warm_make_way(&mut self, kind: usize, now: u64) {
        if self.warm_free(kind) < self.kinds[kind].cfg.slots {
            return;
        }
        let oldest = self.warm.units.iter().filter(|(_, w)| w.kind == kind && w.reserved.is_none());
        if let Some(u) = oldest.min_by_key(|(_, w)| w.made_ms).map(|(u, _)| u.clone()) {
            self.end_warm(&u, End::Evict, now);
        }
    }

    fn warm_backoff(&mut self, kind: usize, row: &str, now: u64) {
        let payload = self.warm_payload(kind, row, (self.now_unix)()).ok();
        let (fails, at, seen) = self.warm.backoff.entry((kind, row.to_string())).or_insert((0, 0, None));
        *fails += 1;
        let wait = if *fails >= 3 { PAUSE_MS } else { (BACKOFF_MS << (*fails - 1)).min(PAUSE_MS) };
        *at = now + wait;
        *seen = payload;
    }

    /// End the warm unit `unit`: kill, destroy (a residue makes the host
    /// `degraded`, as for a job unit), close its sockets, and write its
    /// final line. A reserved unit gives its job a cold unit (P2-1).
    pub(crate) fn end_warm(&mut self, unit: &UnitId, end: End, now: u64) {
        let Some(w) = self.warm.units.remove(unit) else { return };
        let d = self.kinds[w.kind].driver.clone();
        let _ = d.signal(unit, Signal::Kill);
        match d.destroy(unit) {
            Ok(r) if r.is_clean() => {}
            Ok(r) => self.degraded = Some(format!("{unit} left {:?} processes and {:?} files", r.processes, r.files)),
            Err(e) => self.degraded = Some(format!("{unit}: destroy: {e}")),
        }
        self.close_sockets(unit);
        let state = if end.is_failure() { State::Failed } else { State::Released };
        self.warm_line(unit, w.kind, state, now);
        self.audit_host(unit, format!("end reason={}", end.word()));
        if end.is_failure() {
            self.warm_backoff(w.kind, &w.row, now);
        }
        if let Some(key) = w.reserved
            && end != End::JobEnded
        {
            let cold = self.ids.next();
            if let Some(j) = self.jobs.get_mut(&key) {
                j.unit = Some(cold);
            }
        }
    }

    /// A journal line of the warm unit `unit`. False: not written (the
    /// host is then `degraded`).
    pub(crate) fn warm_line(&mut self, unit: &UnitId, kind: usize, state: State, now: u64) -> bool {
        let Ok(job_id) = JobId::new(unit.as_str()) else { return false };
        let line = Line {
            t_ms: now,
            source: WARM_SOURCE.into(),
            job_id,
            state,
            unit_id: Some(unit.clone()),
            unit_kind: Some(self.kinds[kind].name.clone()),
            reason: None,
            code: None,
            exit_code: None,
        };
        if self.journal.write(&line).is_err() {
            self.degraded = Some("the journal cannot be written".into());
            return false;
        }
        true
    }

    /// An audit line of the host (`warm`): to `audit.jsonl` only.
    pub(crate) fn audit_host(&mut self, unit: &UnitId, detail: String) {
        let Ok(job_id) = JobId::new(unit.as_str()) else { return };
        let e = audit::Entry {
            seq: 0,
            prev: String::new(),
            t_ms: self.now(),
            audit: audit::Kind::Warm,
            source: WARM_SOURCE.into(),
            job_id,
            code: None,
            detail: Some(detail),
        };
        let _ = self.audit.write(e);
    }
}
