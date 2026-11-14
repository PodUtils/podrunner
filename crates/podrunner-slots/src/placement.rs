//! Placement: which unit kind takes an offer, or why none does (GitHub
//! #21). The steps run in order; the first step that leaves no candidate
//! gives the refusal code.

use podrunner_schema::{
    Boundary, Capability, ContainerRoute, EgressHold, JobOffer, LimitMode, Principal, RefusalCode, SecretDelivery, Tier,
};

use crate::config::Config;

/// One unit kind of the host, as placement sees it.
#[derive(Debug, Clone)]
pub struct Kind {
    pub capability: Capability,
    /// The slots of the kind that are free now.
    pub free: u32,
    /// The units of the kind that run now.
    pub running: u32,
    /// The repositories (`owner/repo`) of the running units.
    pub repos: Vec<String>,
}

/// Where an offer goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// The index of the kind in the list.
    pub kind: usize,
    pub tier: Tier,
    pub boundary: Boundary,
    /// The pinned reference of the payload.
    pub payload: String,
    /// True when the offer named a payload that placement did not take
    /// (an audit line `payload-ignored`).
    pub payload_ignored: bool,
}

/// The hold that a tier needs.
pub fn needed_hold(t: Tier) -> EgressHold {
    match t {
        Tier::Untrusted => EgressHold::Enforced,
        Tier::Trusted => EgressHold::Advisory,
        Tier::Local => EgressHold::None,
    }
}

/// Keep the candidates that `keep` accepts; when none is left, refuse with
/// `code`.
fn narrow(
    c: &mut Vec<usize>,
    kinds: &[Kind],
    code: RefusalCode,
    keep: impl Fn(&Kind) -> bool,
) -> Result<(), RefusalCode> {
    c.retain(|i| keep(&kinds[*i]));
    if c.is_empty() { Err(code) } else { Ok(()) }
}

/// Place an offer of a source of tier `set`. `now_s` is the time in
/// seconds of the Unix epoch, for the age of the payload.
pub fn place(offer: &JobOffer, set: Tier, kinds: &[Kind], cfg: &Config, now_s: u64) -> Result<Placement, RefusalCode> {
    let mut c: Vec<usize> = (0..kinds.len()).collect();
    narrow(&mut c, kinds, RefusalCode::ArchMismatch, |k| k.capability.arch == offer.arch)?;
    let tier = podrunner_tier::intake(&offer.trust_inputs, &cfg.trust, set)?;
    let repo = format!("{}/{}", offer.trust_inputs.owner, offer.trust_inputs.repo);
    // One repository for the host (DR-4): the files of the uid outlive a job.
    if cfg.repository.as_ref().is_some_and(|r| !r.eq_ignore_ascii_case(&repo)) {
        return Err(RefusalCode::RepoScopeRefused);
    }
    let need = offer.unit_need.clone().unwrap_or_default();
    let boundary = cfg.floor(tier).max(need.min_boundary.unwrap_or(Boundary::None));
    let no_boundary =
        if tier == Tier::Untrusted { RefusalCode::TierNeedsGuest } else { RefusalCode::BoundaryNotAvailable };
    narrow(&mut c, kinds, no_boundary, |k| k.capability.boundary >= boundary)?;
    narrow(&mut c, kinds, RefusalCode::LimitNotEnforceable, |k| {
        offer.limits_required.iter().all(|l| k.capability.limit(l) == LimitMode::Enforced)
    })?;
    narrow(&mut c, kinds, RefusalCode::HoldNotAvailable, |k| {
        k.capability.egress_hold.effective() >= needed_hold(tier)
    })?;
    // podrunner delivers a credential as files (call C13).
    narrow(&mut c, kinds, RefusalCode::BoundaryNotAvailable, |k| {
        k.capability.secret_delivery.contains(&SecretDelivery::Files)
    })?;
    // A `container:` job runs through the hooks or in the engine of the
    // unit; `services:` and a Dockerfile action need the engine (T-103).
    if need.container {
        narrow(&mut c, kinds, RefusalCode::ContainerNotSupported, |k| {
            matches!(k.capability.container, ContainerRoute::Hooks | ContainerRoute::Engine)
        })?;
    }
    if need.container_engine {
        narrow(&mut c, kinds, RefusalCode::ContainerNotSupported, |k| {
            k.capability.container == ContainerRoute::Engine
        })?;
    }
    // A policy of the offer needs the egress module on the way out of the
    // unit, and no path rule: podrunner has no TLS end (T-107).
    if !offer.policy.is_empty() {
        if crate::policy::has_path_rule(&offer.policy) {
            return Err(RefusalCode::EgressPolicyUnsupported);
        }
        narrow(&mut c, kinds, RefusalCode::EgressPolicyUnsupported, |k| {
            k.capability.egress_hold.effective() > EgressHold::None
        })?;
    }
    let (payload, payload_ignored) = pick_payload(offer, cfg, now_s)?;
    // A shared principal runs one unit at a time; with same_principal_ok,
    // more, but of one repository only (affinity).
    let free = |k: &Kind| {
        k.free > 0
            && match (k.capability.principal, cfg.same_principal_ok) {
                (Principal::Shared, false) => k.running == 0,
                (Principal::Shared, true) => k.repos.iter().all(|r| *r == repo),
                _ => true,
            }
    };
    let kind = *c.iter().find(|i| free(&kinds[**i])).ok_or(RefusalCode::NoSlotOfKind)?;
    Ok(Placement { kind, tier, boundary, payload, payload_ignored })
}

/// The payload of the pin list for the architecture of the offer.
fn pick_payload(offer: &JobOffer, cfg: &Config, now_s: u64) -> Result<(String, bool), RefusalCode> {
    let pinned = |reference: &str| {
        cfg.pin.payload.iter().find(|p| {
            p.arch == offer.arch && podrunner_unit::parse_pinned(&p.reference).is_ok_and(|(_, d)| d == reference)
        })
    };
    let from_offer = offer.payload.as_deref().filter(|_| cfg.payload_from_offer);
    let chosen = match from_offer {
        Some(digest) => pinned(digest).ok_or(RefusalCode::PayloadNotPinned)?,
        None => cfg.pin.payload.iter().find(|p| p.arch == offer.arch).ok_or(RefusalCode::PayloadNotPinned)?,
    };
    if chosen.stale(&cfg.pin, now_s) {
        return Err(RefusalCode::ImageStale);
    }
    Ok((chosen.reference.clone(), offer.payload.is_some() && from_offer.is_none()))
}
