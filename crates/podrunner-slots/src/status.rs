//! `status`: what the supervisor holds now, by source, then by tier, and
//! each unit kind with its record (`status.v1`, GitHub #22).

use std::collections::BTreeMap;

use podrunner_schema::{Capability, Tier};
use serde::Serialize;

use crate::engine::Slots;
use crate::state::State;

/// One source in the status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct SourceStatus {
    pub name: String,
    pub tier: Tier,
    pub arch: String,
    pub revoked: bool,
    /// The jobs of the source in each state that is not final.
    pub jobs: BTreeMap<String, u64>,
    /// The refusals of the source, by code.
    pub refused: BTreeMap<String, u64>,
    /// The offers whose trust inputs hold `unknown`: the source could not
    /// read the run or the membership.
    pub trust_unknown: u64,
    /// The age of the credential of each job that is not final, in
    /// seconds, oldest first (SPEC §11.5).
    pub credential_ages_s: Vec<u64>,
    /// The age of the key file of the source (the App key of
    /// `github-scaleset`, the key of `remote`), from its time of change.
    /// No value of the key is read.
    pub key_age_s: Option<u64>,
}

/// One unit kind in the status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct KindStatus {
    pub name: String,
    pub slots: u32,
    pub busy: u32,
    pub probe: Capability,
}

/// One pinned payload in the status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct PinStatus {
    pub arch: String,
    pub reference: String,
    pub age_s: u64,
}

/// The status of a host (`status.v1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct Status {
    pub schema: u32,
    pub host: String,
    pub draining: bool,
    /// Why the host is degraded: capacity 0 until an operator clears it.
    pub degraded: Option<String>,
    /// No unit kind of the host meets the floor of any source: no source
    /// gets an offer.
    pub no_tier_served: bool,
    pub sources: Vec<SourceStatus>,
    pub kinds: Vec<KindStatus>,
    pub pins: Vec<PinStatus>,
    /// The jobs that ended without a `completed` event of their source.
    pub ended_without_completion: u64,
    /// The reports that a source refused, in the spool (T-069).
    pub spooled_reports: usize,
}

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
                    if j.offer.as_ref().is_some_and(|o| {
                        let t = &o.trust_inputs;
                        serde_json::to_value(t)
                            .is_ok_and(|v| v.as_object().is_some_and(|m| m.values().any(|x| x == "unknown")))
                    }) {
                        trust_unknown += 1;
                    }
                    if let (false, Some(m)) = (j.state.is_final(), j.minted_ms) {
                        credential_ages_s.push(self.clock.now_ms().saturating_sub(m) / 1000);
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
            .map(|(k, v)| KindStatus {
                name: k.name.clone(),
                slots: k.cfg.slots,
                busy: v.running,
                probe: k.capability.clone(),
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
            spooled_reports: self.spooled(),
            sources,
            kinds,
            pins,
            // A unit ended and was reported, and the source never said
            // that the job completed (no retry of a lost job).
            ended_without_completion: self
                .jobs
                .values()
                .filter(|j| j.state == State::Reported && j.unit.is_some() && !j.completed)
                .count() as u64,
        }
    }

    /// True when no kind meets the floor of the tier of any source.
    pub fn no_tier_served(&self) -> bool {
        !self.sources.iter().any(|s| {
            let floor = self.cfg.floor(s.cfg.tier);
            self.kinds.iter().any(|k| k.capability.arch == s.cfg.arch && k.capability.boundary >= floor)
        })
    }
}
