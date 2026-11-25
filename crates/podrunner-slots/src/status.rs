//! `status`: what the supervisor holds now, by source, then by tier, and
//! each unit kind with its record (`status.v1`, GitHub #22).

use std::collections::BTreeMap;

use crate::engine::Slots;
use crate::state::State;

/// The types of `status.v1` are `pod_job`'s (podcrates `v0.73.0`, T-508):
/// one form for podrunner and each reader (githome's G-31).
pub use podrunner_schema::{KindStatus, PinStatus, SourceStatus, Status, WarmStatus};

/// The age of the file `path` from its time of change, in seconds.
pub(crate) fn key_age_s(path: &str, now_unix: u64) -> Option<u64> {
    let changed = std::fs::metadata(path).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(now_unix.saturating_sub(changed.as_secs()))
}

fn state_name(s: State) -> String {
    serde_json::to_value(s).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

impl Slots {
    pub fn status(&self) -> Status {
        let views = self.kind_views();
        let sources = self
            .sources
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let mut jobs = BTreeMap::new();
                let mut refused = BTreeMap::new();
                let mut trust_unknown = 0;
                let mut credential_ages_s = Vec::new();
                for j in self.jobs.values().filter(|j| j.source == i) {
                    // Only the fields with an `unknown` value of their enum: a
                    // text field (an owner, a ref) named `unknown` is known (#285).
                    if j.offer.as_ref().is_some_and(|o| {
                        serde_json::to_value(&o.trust_inputs).is_ok_and(|v| {
                            ["event", "head_is_fork", "actor_class", "repo_visibility"]
                                .iter()
                                .any(|k| v[*k] == "unknown")
                        })
                    }) {
                        trust_unknown += 1;
                    }
                    if let (false, Some(m)) = (j.state.is_final(), j.minted_ms) {
                        credential_ages_s.push(self.now().saturating_sub(m) / 1000);
                    }
                    if !j.state.is_final() {
                        *jobs.entry(state_name(j.state)).or_insert(0) += 1;
                    }
                    if let Some(c) = j.refusal {
                        *refused.entry(c.as_str().to_string()).or_insert(0) += 1;
                    }
                }
                credential_ages_s.sort_unstable_by(|a, b| b.cmp(a));
                SourceStatus {
                    key_age_s: s.cfg.key_file.as_deref().and_then(|k| key_age_s(k, (self.now_unix)())),
                    credential_ages_s,
                    name: s.cfg.name.clone(),
                    tier: s.cfg.tier,
                    arch: s.cfg.arch.clone(),
                    revoked: s.revoked,
                    draining: self.drained.contains(&s.cfg.name),
                    jobs,
                    refused,
                    trust_unknown,
                }
            })
            .collect();
        let kinds = self
            .kinds
            .iter()
            .zip(&views)
            .enumerate()
            .map(|(i, (k, v))| KindStatus {
                name: k.name.clone(),
                // A shared principal runs one unit at a time unless
                // `same_principal_ok` (placement): the slots that it has
                // (T-185).
                slots: if k.capability.principal == podrunner_schema::Principal::Shared && !self.cfg.same_principal_ok {
                    k.cfg.slots.min(1)
                } else {
                    k.cfg.slots
                },
                busy: v.running,
                probe: k.capability.clone(),
                warm: self.warm_status(i),
            })
            .collect();
        let now = (self.now_unix)();
        let pins = self
            .cfg
            .pin
            .payload
            .iter()
            .map(|p| PinStatus {
                arch: p.arch.clone(),
                reference: p.reference.clone(),
                age_s: now.saturating_sub(p.built_at_s),
            })
            .collect();
        Status {
            schema: podrunner_schema::SCHEMA,
            host: self.cfg.host.clone(),
            draining: self.draining,
            degraded: self.degraded.clone(),
            no_tier_served: self.no_tier_served(),
            max_units: self.cfg.max_units,
            units: self.jobs.values().filter(|j| j.holds_unit()).count() as u32,
            spooled_reports: self.spooled(),
            sources,
            kinds,
            pins,
            warm_units: self.warm_units() as u32,
            // A unit ended and was reported, and the source never said
            // that the job completed (no retry of a lost job).
            ended_without_completion: self
                .jobs
                .values()
                .filter(|j| j.state == State::Reported && j.unit.is_some() && !j.completed)
                .count() as u64,
        }
    }

    /// The target and available warm units of one kind.
    fn warm_status(&self, kind: usize) -> Option<WarmStatus> {
        let target: u32 = self.kinds[kind].cfg.warm_rows().iter().map(|(_, n)| n).sum();
        let mut units = self.warm.units.values().filter(|w| w.kind == kind).peekable();
        if target == 0 && units.peek().is_none() {
            return None;
        }
        let (ready, booting) = units.filter(|w| w.reserved.is_none()).fold((0, 0), |(ready, booting), w| {
            if w.ready_ms.is_some() { (ready + 1, booting) } else { (ready, booting + 1) }
        });
        Some(WarmStatus { target, booting, ready, paused_reason: self.warm_paused(kind) })
    }

    /// Why the refill boots no new warm unit of `kind` now (`warm_refill`).
    fn warm_paused(&self, kind: usize) -> Option<String> {
        if self.draining {
            return Some("the host drains".into());
        }
        if self.degraded.is_some() {
            return Some("the host is degraded".into());
        }
        if self.egress_down {
            return Some("the egress module ended".into());
        }
        let now = self.now();
        let wait = self.warm.backoff.iter().find(|((k, _), (_, at, _))| *k == kind && *at > now);
        if let Some(((_, row), (fails, at, _))) = wait {
            let s = (at - now).div_ceil(1000);
            return Some(format!("the row {row:?} failed to boot {fails} times in a row; the next try in {s} s"));
        }
        let unit_mem = crate::limits::limits_of(&self.kinds[kind].cfg).ok().and_then(|l| l.memory_bytes).unwrap_or(0);
        if (self.warm.mem_probe)().is_some_and(|a| a < unit_mem.saturating_add(self.warm_reserve())) {
            return Some("the free memory of the host is under the memory of a unit and warm_reserve".into());
        }
        None
    }

    /// True when no kind meets the floor of the tier of any source.
    pub fn no_tier_served(&self) -> bool {
        !self.sources.iter().any(|s| {
            // The floor and the hold, as placement needs them (#267).
            let (floor, hold) = (self.cfg.floor(s.cfg.tier), crate::placement::needed_hold(s.cfg.tier));
            self.kinds.iter().any(|k| {
                k.capability.arch == s.cfg.arch
                    && k.capability.boundary >= floor
                    && k.capability.egress_hold.effective() >= hold
            })
        })
    }
}
