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
                for j in self.jobs.values().filter(|j| j.source == i) {
                    if j.offer.as_ref().is_some_and(|o| {
                        let t = &o.trust_inputs;
                        serde_json::to_value(t)
                            .is_ok_and(|v| v.as_object().is_some_and(|m| m.values().any(|x| x == "unknown")))
                    }) {
                        trust_unknown += 1;
                    }
                    if !j.state.is_final() {
                        *jobs.entry(state_name(j.state)).or_insert(0) += 1;
                    }
                    if let Some(c) = j.refusal {
                        *refused.entry(c.as_str().to_string()).or_insert(0) += 1;
                    }
                }
                SourceStatus {
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
