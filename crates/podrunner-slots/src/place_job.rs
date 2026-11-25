//! The place of a waiting offer: placement, the unit id and the runner
//! name, or a refusal that withdraws the offer.

use podrunner_schema::{ExitReason, JobId, LimitMode, RefusalCode};

use crate::audit;
use crate::engine::{Job, Slots};
use crate::placement;
use crate::state::State;

impl Slots {
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
        let trust = self.cfg.trust_for(&self.sources[s].cfg, &offer.trust_inputs);
        let now_s = (self.now_unix)();
        // Each credential of the job names a secret of this host for its
        // source, its hosts and its places (T-563).
        let secrets = crate::secrets::check_job(&offer.policy, &self.cfg.egress, &self.sources[s].cfg.name)
            .map_err(|d| placement::Refusal { code: RefusalCode::EgressPolicyUnsupported, detail: Some(d) });
        match trust
            .map_err(placement::Refusal::from)
            .and_then(|t| secrets.map(|()| t))
            .and_then(|t| placement::place_why(&offer, set, &self.kind_views(), &self.cfg, t, now_s))
        {
            Ok(p) => {
                let tier = format!("{:?}", p.tier).to_lowercase();
                self.audit(audit::Kind::Tier, s, &key.1, None, Some(tier));
                if p.payload_ignored {
                    self.audit(audit::Kind::PayloadIgnored, s, &key.1, None, None);
                }
                // A free warm unit that fits, else a cold one (T-487).
                let unit = self.unit_for(key, &p, &offer, now);
                let runner = format!("podrunner-{}-{unit}", self.cfg.host);
                if let Some(j) = self.jobs.get_mut(key) {
                    j.placement = Some(p);
                    j.unit = Some(unit);
                    j.runner_name = Some(runner);
                }
                self.set(key, State::Matched, now);
                self.claim(key, now);
            }
            Err(r) if r.code.is_wait() => {}
            Err(placement::Refusal { code, detail: Some(d) }) => self.refuse_naming(key, code, Some(d), now),
            Err(placement::Refusal { code, detail: None }) => {
                // A limit refusal names the limits that no kind enforces.
                let detail = (code == RefusalCode::LimitNotEnforceable).then(|| {
                    let views = self.kind_views();
                    let missing: Vec<&str> = offer
                        .limits_required
                        .iter()
                        .filter(|l| !views.iter().any(|k| k.capability.limit(l) == LimitMode::Enforced))
                        .map(String::as_str)
                        .collect();
                    // Each limit has a kind, but no kind has each one.
                    if missing.is_empty() { offer.limits_required.join(",") } else { missing.join(",") }
                });
                self.refuse_naming(key, code, detail, now)
            }
        }
    }
    /// A terminal refusal: withdraw the offer, report it, and suppress the
    /// job id; `detail` goes to the audit line.
    pub(crate) fn refuse_naming(&mut self, key: &(usize, JobId), code: RefusalCode, detail: Option<String>, now: u64) {
        self.refuse_as(key, code, code.as_str(), detail, now)
    }

    /// `refuse_naming`, with the reason of the withdraw that the source gets.
    pub(crate) fn refuse_as(
        &mut self,
        key: &(usize, JobId),
        code: RefusalCode,
        reason: &str,
        detail: Option<String>,
        now: u64,
    ) {
        let s = key.0;
        let _ = self.sources[s].source.withdraw(&key.1, reason);
        if let Some(j) = self.jobs.get_mut(key) {
            j.refusal = Some(code);
            j.reason = Some(ExitReason::Refused);
            j.unit = None;
            j.placement = None;
        }
        self.audit(audit::Kind::Refuse, s, &key.1, Some(code), detail);
        self.suppressed.insert(key.clone(), now + self.cfg.terminal_suppress_s * 1000);
        self.report(key);
        self.set(key, State::Refused, now);
    }
}
