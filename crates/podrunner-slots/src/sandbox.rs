//! A sandbox unit (T-506, githome's G-36): an offer with `sandbox { image,
//! command, idle_s, max_s }` runs the command of the source in a unit of
//! the image, with the isolation, the egress hold, the limits and the
//! report of a job unit, and no runner.
//!
//! The calls of Q-7 (`TODO/later.md`, T-506): a sandbox runs at
//! `untrusted` (a guest), also for a member; its image is a pin of the
//! operator marked `sandbox = true`, which a runner job never takes; the
//! payload carries its own attach client, so podrunner gives it `idle_s`
//! (`PODRUNNER_IDLE_S`) and ends the unit at `max_s`. An attach is one long
//! connection through the egress module: podrunner sees no idle time of it.

use std::collections::BTreeMap;

use podrunner_schema::{JobOffer, RefusalCode, Tier};

use crate::config::{Config, Payload};

/// The variable that gives the payload of a sandbox its `idle_s`.
pub const IDLE_VAR: &str = "PODRUNNER_IDLE_S";

/// A sandbox runs at `untrusted`; another offer at the tier of its intake.
pub(crate) fn tier(offer: &JobOffer, tier: Tier) -> Tier {
    if offer.sandbox.is_some() { Tier::Untrusted } else { tier }
}

/// The digest of a `NAME@sha256:HEX`.
fn digest(reference: &str) -> Option<&str> {
    podrunner_unit::parse_pinned(reference).ok().map(|(_, d)| d)
}

/// For a sandbox offer, the pin of its image: a `[[pin.payload]]` with
/// `sandbox = true` of the architecture and the digest of the offer, else
/// `payload-not-pinned`. None for another offer.
pub(crate) fn pin<'a>(offer: &JobOffer, cfg: &'a Config) -> Option<Result<&'a Payload, RefusalCode>> {
    let want = digest(&offer.sandbox.as_ref()?.image);
    let found = cfg
        .pin
        .payload
        .iter()
        .find(|p| p.sandbox && p.arch == offer.arch && want.is_some() && digest(&p.reference) == want);
    Some(found.ok_or(RefusalCode::PayloadNotPinned))
}

/// The argv of the unit: the command of a sandbox, else the command of the
/// kind (the runner).
pub(crate) fn argv(offer: Option<&JobOffer>, kind: &[String]) -> Vec<String> {
    offer.and_then(|o| o.sandbox.as_ref()).map_or_else(|| kind.to_vec(), |s| s.command.clone())
}

/// The environment of a sandbox: its `idle_s`.
pub(crate) fn env(offer: Option<&JobOffer>, env: &mut BTreeMap<String, String>) {
    if let Some(i) = offer.and_then(|o| o.sandbox.as_ref()).and_then(|s| s.idle_s) {
        env.insert(IDLE_VAR.into(), i.to_string());
    }
}

/// The time of the run of a unit: the lesser of `wall_s` and the `max_s`
/// of a sandbox.
pub(crate) fn wall_s(offer: Option<&JobOffer>, wall_s: u64) -> u64 {
    offer.and_then(|o| o.sandbox.as_ref()).and_then(|s| s.max_s).map_or(wall_s, |m| m.min(wall_s))
}
