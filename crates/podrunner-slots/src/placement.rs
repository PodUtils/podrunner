//! Placement: which unit kind takes an offer, or why none does (GitHub
//! #21). The steps run in order; the first step that leaves no candidate
//! gives the refusal code.

use podrunner_schema::{
    Boundary, Capability, ContainerRoute, EgressHold, JobOffer, LimitMode, Principal, RefusalCode, SecretDelivery, Tier,
};

use crate::config::{Config, Payload, UnitInit, init_name};

/// One unit kind of the host, as placement sees it.
#[derive(Debug, Clone)]
pub struct Kind {
    pub capability: Capability,
    /// The engine capability depends on the selected payload.
    pub engine_needs_payload: bool,
    /// The slots of the kind that are free now.
    pub free: u32,
    /// The units of the kind that run now.
    pub running: u32,
    /// The repositories (`owner/repo`) of the running units.
    pub repos: Vec<String>,
    /// With `root_quota`, the size of the root disk of a unit:
    /// `disk_max_bytes`, or `u64::MAX` with none; the `disk` limit of an
    /// offer narrows it (T-482). `None`: the kind does not size the root.
    pub root_disk: Option<u64>,
    /// The PID 1 of a unit (`[unit.KIND] init`): a payload goes only to a
    /// kind with its own `init` (T-549).
    pub init: Option<UnitInit>,
    /// The payload of each free warm unit of the kind (T-487): among the
    /// kinds that meet a job, one with a warm unit of its payload comes
    /// first.
    pub warm: Vec<String>,
}

/// A refusal of placement, with a detail for the audit line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: RefusalCode,
    pub detail: Option<String>,
}

impl From<RefusalCode> for Refusal {
    fn from(code: RefusalCode) -> Refusal {
        Refusal { code, detail: None }
    }
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
    /// The pin declares a runner hook that checks the job before its first step.
    pub assignment_hook: bool,
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
    place_for(offer, set, kinds, cfg, &cfg.trust, now_s)
}

/// [`place`] with the trust lists `trust` of the source of the offer
/// (T-494).
pub fn place_for(
    offer: &JobOffer,
    set: Tier,
    kinds: &[Kind],
    cfg: &Config,
    trust: &podrunner_tier::TierConfig,
    now_s: u64,
) -> Result<Placement, RefusalCode> {
    place_why(offer, set, kinds, cfg, trust, now_s).map_err(|r| r.code)
}

/// [`place_for`], with the detail of a refusal.
pub fn place_why(
    offer: &JobOffer,
    set: Tier,
    kinds: &[Kind],
    cfg: &Config,
    trust: &podrunner_tier::TierConfig,
    now_s: u64,
) -> Result<Placement, Refusal> {
    let mut c: Vec<usize> = (0..kinds.len()).collect();
    narrow(&mut c, kinds, RefusalCode::ArchMismatch, |k| k.capability.arch == offer.arch)?;
    let tier = crate::sandbox::tier(offer, podrunner_tier::intake(&offer.trust_inputs, trust, set)?);
    let repo = format!("{}/{}", offer.trust_inputs.owner, offer.trust_inputs.repo);
    // One repository for the host (DR-4): the files of the uid outlive a job.
    if cfg.repository.as_ref().is_some_and(|r| !r.eq_ignore_ascii_case(&repo)) {
        return Err(RefusalCode::RepoScopeRefused.into());
    }
    let need = offer.unit_need.clone().unwrap_or_default();
    let boundary = cfg.floor(tier).max(need.min_boundary.unwrap_or(Boundary::None));
    let no_boundary =
        if tier == Tier::Untrusted { RefusalCode::TierNeedsGuest } else { RefusalCode::BoundaryNotAvailable };
    narrow(&mut c, kinds, no_boundary, |k| k.capability.boundary >= boundary)?;
    narrow(&mut c, kinds, RefusalCode::LimitNotEnforceable, |k| {
        // Enforced or reported, as SPEC 9 says (#248); a `wall` also needs a
        // verified tree kill, which `limit` does not say for `reported`.
        offer.limits_required.iter().all(|l| {
            matches!(k.capability.limit(l), LimitMode::Enforced | LimitMode::Reported)
                && (l != "wall" || k.capability.tree_kill.is_verified())
        })
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
        if crate::policy::unheld_rule(&offer.policy) {
            return Err(RefusalCode::EgressPolicyUnsupported.into());
        }
        narrow(&mut c, kinds, RefusalCode::EgressPolicyUnsupported, |k| {
            k.capability.egress_hold.effective() > EgressHold::None
        })?;
    }
    // A unit of a job with a credential does not run as the user of
    // podrunner, who reads the files of the secrets and the memory of the
    // broker: a kind with its own namespaces or root (`confined` or more),
    // or with a distinct user (T-563).
    if crate::secrets::has_credentials(&offer.policy)
        || cfg.egress.default_policy.as_ref().is_some_and(crate::secrets::has_credentials)
    {
        narrow(&mut c, kinds, RefusalCode::EgressPolicyUnsupported, |k| {
            k.capability.boundary >= Boundary::Confined || k.capability.principal == Principal::Distinct
        })?;
    }
    let (pin, payload_ignored) = pick_payload(offer, cfg, now_s)?;
    // A payload that boots its own init (systemd) goes only to a kind that
    // boots it, and a payload with no init only to a kind that does not
    // (T-549): the unit would start and fail at its PID 1.
    if !c.iter().any(|i| kinds[*i].init == pin.init) {
        return Err(Refusal {
            code: RefusalCode::BoundaryNotAvailable,
            detail: Some(format!(
                "init: the payload {} needs {}; no kind that meets the job has it",
                pin.reference,
                init_name(pin.init)
            )),
        });
    }
    c.retain(|i| kinds[*i].init == pin.init);
    if (need.container || need.container_engine) && !pin.container_engine {
        narrow(&mut c, kinds, RefusalCode::ContainerNotSupported, |k| !k.engine_needs_payload)?;
    }
    // A root disk smaller than the image cannot hold it (T-571): the unit
    // would start and fail with no clear cause.
    if let Some(size) = pin.unpacked_bytes {
        let offered = offer.limits.get("disk").copied().unwrap_or(u64::MAX);
        let root = |k: &Kind| k.root_disk.map(|r| r.min(offered));
        if let Some(bound) = c.iter().filter_map(|i| root(&kinds[*i])).filter(|b| *b < size).max() {
            c.retain(|i| root(&kinds[*i]).is_none_or(|b| b >= size));
            if c.is_empty() {
                return Err(Refusal {
                    code: RefusalCode::LimitNotEnforceable,
                    detail: Some(format!(
                        "disk: the root disk of a unit is {bound} bytes (the lesser of the disk limit of the offer \
                         and disk_max_bytes), less than the {size} bytes of the payload {} unpacked",
                        pin.reference
                    )),
                });
            }
        }
    }
    // A shared principal runs one unit at a time; with same_principal_ok,
    // more, but of one repository only (affinity).
    let free = |k: &Kind| {
        k.free > 0
            && match (k.capability.principal, cfg.same_principal_ok) {
                (Principal::Shared, false) => k.running == 0,
                // GitHub compares an owner and a repository with no case (#236).
                (Principal::Shared, true) => k.repos.iter().all(|r| r.eq_ignore_ascii_case(&repo)),
                _ => true,
            }
    };
    let first = *c.iter().find(|i| free(&kinds[**i])).ok_or(RefusalCode::NoSlotOfKind)?;
    let warm = c.iter().find(|i| free(&kinds[**i]) && kinds[**i].warm.contains(&pin.reference));
    let kind = warm.copied().unwrap_or(first);
    Ok(Placement {
        kind,
        tier,
        boundary,
        payload: pin.reference.clone(),
        assignment_hook: pin.assignment_hook,
        payload_ignored,
    })
}

/// The payload of the pin list for the architecture of the offer.
fn pick_payload<'a>(offer: &JobOffer, cfg: &'a Config, now_s: u64) -> Result<(&'a Payload, bool), RefusalCode> {
    // The image of a sandbox (T-506).
    if let Some(pin) = crate::sandbox::pin(offer, cfg) {
        let pin = pin?;
        if pin.stale(&cfg.pin, now_s) {
            return Err(RefusalCode::ImageStale);
        }
        return Ok((pin, false));
    }
    let pinned = |reference: &str| {
        cfg.pin.payload.iter().find(|p| {
            !p.sandbox
                && p.arch == offer.arch
                && (p.labels.is_empty() || p.names_label(&offer.labels))
                && podrunner_unit::parse_pinned(&p.reference).is_ok_and(|(_, d)| d == reference)
        })
    };
    let from_offer = offer.payload.as_deref().filter(|_| cfg.payload_from_offer);
    let chosen = match from_offer {
        Some(digest) => pinned(digest).ok_or(RefusalCode::PayloadNotPinned)?,
        None => return pick_pinned(cfg, &offer.arch, &offer.labels, now_s).map(|p| (p, offer.payload.is_some())),
    };
    if chosen.stale(&cfg.pin, now_s) {
        return Err(RefusalCode::ImageStale);
    }
    Ok((chosen, false))
}

/// The payload of the pin list for `arch` and the `runs-on` labels (T-549:
/// a payload that names a label of the job comes first; a payload that
/// names labels serves only them): the
/// first one that is not stale (a pin list keeps the old payload beside the
/// new one, #235). A warm unit of a label row gets its payload by the same
/// rule (T-487).
pub(crate) fn pick_pinned<'a>(
    cfg: &'a Config,
    arch: &str,
    labels: &[String],
    now_s: u64,
) -> Result<&'a Payload, RefusalCode> {
    let named = cfg.pin.payload.iter().any(|p| !p.sandbox && p.arch == arch && p.names_label(labels));
    let serves = |p: &Payload| if named { p.names_label(labels) } else { p.labels.is_empty() };
    let mut of_arch = cfg.pin.payload.iter().filter(|p| !p.sandbox && p.arch == arch && serves(p)).peekable();
    let first = *of_arch.peek().ok_or(RefusalCode::PayloadNotPinned)?;
    let chosen = of_arch.find(|p| !p.stale(&cfg.pin, now_s)).unwrap_or(first);
    if chosen.stale(&cfg.pin, now_s) {
        return Err(RefusalCode::ImageStale);
    }
    Ok(chosen)
}
