//! The tier rule (GitHub #18). Trust comes from the metadata of a job,
//! never from a runner label: the author of a workflow chooses `runs-on`,
//! so a job from a fork can name the trusted set. The rule denies by
//! default.
//!
//! - `trusted` when each holds: the repository is private or internal, or
//!   is in `trusted_public`; the owner is a configured organization; the
//!   head is not a fork; the actor is a member; the event is `push`,
//!   `pull_request`, `workflow_dispatch`, `schedule` or `merge_group`.
//! - `untrusted` when the job is not trusted, the event is `pull_request`,
//!   and the head is a fork.
//! - Else `event-not-allowed`: this holds `pull_request_target`,
//!   `workflow_run`, `issue_comment`, `release`, `repository_dispatch`,
//!   `other`, each `unknown` value, a pull request from the same
//!   repository by an actor that is not a member, and an event of a public
//!   repository that is not listed.
//! - `local` is never computed: only the configuration marks a set
//!   `local`.
//!
//! The intake check: a computed tier less trusted than the tier of the set
//! is `tier-mismatch`. A job more trusted than its set runs under the
//! stricter tier of the set. Both refusals come before the claim.

use podrunner_schema::offer::{ActorClass, Event, TrustInputs, Visibility};
use podrunner_schema::{Fact, RefusalCode, Tier};
use serde::{Deserialize, Serialize};

/// A configured organization. With an `id`, an offer that carries
/// `owner_id` must match it too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Organization {
    pub login: String,
    #[serde(default)]
    pub id: Option<u64>,
}

/// A public repository that the operator trusts (`owner/repo`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicRepo {
    pub owner: String,
    pub repo: String,
    #[serde(default)]
    pub id: Option<u64>,
}

/// The configuration that the rule reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierConfig {
    pub organizations: Vec<Organization>,
    #[serde(default)]
    pub trusted_public: Vec<PublicRepo>,
}

/// The events that a trusted job may come from.
const TRUSTED_EVENTS: [Event; 5] =
    [Event::Push, Event::PullRequest, Event::WorkflowDispatch, Event::Schedule, Event::MergeGroup];

/// GitHub logins and repository names do not care about case.
fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// An id of the offer must equal an id of the configuration when both are
/// there.
fn same_id(offer: Option<u64>, config: Option<u64>) -> bool {
    match (offer, config) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

fn configured_owner(t: &TrustInputs, cfg: &TierConfig) -> bool {
    cfg.organizations.iter().any(|o| same(&o.login, &t.owner) && same_id(t.owner_id, o.id))
}

fn listed_public(t: &TrustInputs, cfg: &TierConfig) -> bool {
    cfg.trusted_public.iter().any(|r| same(&r.owner, &t.owner) && same(&r.repo, &t.repo) && same_id(t.repo_id, r.id))
}

/// The tier of a job, or `event-not-allowed`.
pub fn compute(t: &TrustInputs, cfg: &TierConfig) -> Result<Tier, RefusalCode> {
    let visible = match t.repo_visibility {
        Visibility::Private | Visibility::Internal => true,
        Visibility::Public => listed_public(t, cfg),
        Visibility::Unknown => false,
    };
    let trusted = visible
        && configured_owner(t, cfg)
        && t.head_is_fork == Fact::No
        && t.actor_class == ActorClass::Member
        && TRUSTED_EVENTS.contains(&t.event);
    if trusted {
        return Ok(Tier::Trusted);
    }
    if t.event == Event::PullRequest && t.head_is_fork == Fact::Yes {
        return Ok(Tier::Untrusted);
    }
    Err(RefusalCode::EventNotAllowed)
}

/// The order of trust: `untrusted` < `trusted` < `local`.
fn rank(t: Tier) -> u8 {
    match t {
        Tier::Untrusted => 0,
        Tier::Trusted => 1,
        Tier::Local => 2,
    }
}

/// The tier that a job of a set with the tier `set` runs under, or the
/// refusal. A set marked `local` takes no computed job: it serves only a
/// source that the configuration marks local.
pub fn intake(t: &TrustInputs, cfg: &TierConfig, set: Tier) -> Result<Tier, RefusalCode> {
    let computed = compute(t, cfg)?;
    if rank(computed) < rank(set) {
        return Err(RefusalCode::TierMismatch);
    }
    // A job more trusted than its set runs under the stricter tier.
    Ok(set)
}

#[cfg(test)]
mod tests;
