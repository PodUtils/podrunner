//! The tier rule (GitHub #18). Trust comes from the metadata of a job,
//! never from a runner label: the author of a workflow chooses `runs-on`,
//! so a job from a fork can name the trusted set. The rule denies by
//! default.
//!
//! - `trusted` when each holds: the repository is private or internal, or
//!   is in `trusted_public`; the owner is a configured organization or
//!   user; the head is not a fork; the actor is a member; the event is
//!   `push`, `pull_request`, `workflow_dispatch`, `schedule` or
//!   `merge_group`, or an event of [`EXTRA_EVENTS`] that `extra_events`
//!   lists.
//! - `untrusted` when the job is not trusted, the event is `pull_request`,
//!   and the head is a fork; or when an agent made the change and the job
//!   has each other fact of a trusted job (T-106).
//! - Else `event-not-allowed`: this holds `other`, each `unknown` value,
//!   an event of [`EXTRA_EVENTS`] that `extra_events` does not list, a
//!   `pull_request_target` from a fork, an agent's job of another event
//!   from a fork, a pull request from the same repository by an actor that
//!   is not a member, and an event of a public repository that is not
//!   listed.
//! - `local` is never computed: only the configuration marks a set
//!   `local`.
//!
//! The head is a fork when the repository of the head is not the
//! repository of the job (`head_is_fork`, `docs/contracts/source-driver.md`).
//!
//! The intake check: a computed tier less trusted than the tier of the set
//! is `tier-mismatch`. A job more trusted than its set runs under the
//! stricter tier of the set. Both refusals come before the claim.
//!
//! [`TrustRule`] is the seam (DR-3): [`DefaultRule`] is this rule, and
//! [`cases::conformance`] runs the published case table (`tier-cases.json`)
//! against any rule.

use podrunner_schema::offer::{ActorClass, Event, TrustInputs, Visibility};
use podrunner_schema::{Fact, RefusalCode, Tier};
use serde::{Deserialize, Serialize};

/// A configured organization. With an `id`, an offer that carries
/// `owner_id` must match it too.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Organization {
    pub login: String,
    #[serde(default)]
    pub id: Option<u64>,
}

/// A public repository that the operator trusts (`owner/repo`).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicRepo {
    pub owner: String,
    pub repo: String,
    #[serde(default)]
    pub id: Option<u64>,
}

/// The configuration that the rule reads.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierConfig {
    #[serde(default)]
    pub organizations: Vec<Organization>,
    /// Personal accounts whose repositories the operator trusts (T-191): a
    /// forge with repositories of a person (a `program` source). The
    /// GitHub scale set refuses a personal account at its start (call C3)
    /// unless its source sets `allow_personal_account` (T-504).
    #[serde(default)]
    pub users: Vec<Organization>,
    #[serde(default)]
    pub trusted_public: Vec<PublicRepo>,
    /// More events that a trusted job may come from: only those of
    /// [`EXTRA_EVENTS`] (the owner, Q5 and Q12).
    #[serde(default, deserialize_with = "extra_events_of")]
    #[cfg_attr(feature = "schema", schemars(with = "Vec<Event>"))]
    pub extra_events: Vec<Event>,
}

/// The names of [`EXTRA_EVENTS`], for the error of a wrong value (a test
/// keeps them equal).
const EXTRA_NAMES: &[&str] =
    &["workflow_run", "issue_comment", "release", "repository_dispatch", "pull_request_target"];

/// Read `extra_events`. A name that is no event gets an error that lists
/// the events that the key accepts, not each variant of [`Event`]. A name
/// of another event (`other`, `unknown`, `push`) still reads: `check`
/// refuses it with its own text.
fn extra_events_of<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Event>, D::Error> {
    let names = Vec::<String>::deserialize(d)?;
    names
        .into_iter()
        .map(|n| {
            serde_json::from_value::<Event>(serde_json::Value::String(n.clone()))
                .map_err(|_| serde::de::Error::unknown_variant(&n, EXTRA_NAMES))
        })
        .collect()
}

/// The events that a trusted job may come from.
const TRUSTED_EVENTS: [Event; 5] =
    [Event::Push, Event::PullRequest, Event::WorkflowDispatch, Event::Schedule, Event::MergeGroup];

/// The events that the operator may add (the owner, Q5 and Q12). Each runs
/// with the secrets of the repository. On GitHub, `pull_request_target`
/// runs the workflow of the base branch of the pull request (any branch
/// that a pull request can target). `workflow_run`, `issue_comment` and
/// `repository_dispatch` run the workflow of the default branch. `release`
/// runs the workflow of the commit of its tag. `other` and `unknown` are
/// never trusted.
pub const EXTRA_EVENTS: [Event; 5] =
    [Event::WorkflowRun, Event::IssueComment, Event::Release, Event::RepositoryDispatch, Event::PullRequestTarget];

/// The name of an event as the offer and `podrunner.toml` write it.
pub fn event_name(e: Event) -> String {
    serde_json::to_value(e).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| format!("{e:?}"))
}

impl TierConfig {
    /// Refuse an extra event that is not in [`EXTRA_EVENTS`]. An event that
    /// is trusted with no listing does no harm: `notes` names it.
    pub fn check(&self) -> Result<(), String> {
        match self.extra_events.iter().find(|e| !EXTRA_EVENTS.contains(e) && !TRUSTED_EVENTS.contains(e)) {
            Some(e) => Err(format!("[trust] extra_events: {} can never be trusted", event_name(*e))),
            None => Ok(()),
        }
    }

    /// The lines of this table that an operator must know of, one line
    /// each, after the name of the table: a repeated event and an event
    /// that is trusted with no listing (T-642), and the opt-in of
    /// `pull_request_target` (R8).
    pub fn notes(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen: Vec<Event> = Vec::new();
        for e in &self.extra_events {
            if seen.contains(e) {
                out.push(format!("extra_events: {} is listed twice; the second entry does nothing", event_name(*e)));
            } else if TRUSTED_EVENTS.contains(e) {
                out.push(format!(
                    "extra_events: {} is trusted with no listing; the entry does nothing",
                    event_name(*e)
                ));
            }
            seen.push(*e);
        }
        if self.extra_events.contains(&Event::PullRequestTarget) {
            out.push(
                "extra_events: pull_request_target is trusted (residual R8, docs/security/residuals.md): a job of \
                 this event is trusted when its owner is configured, its head is not a fork and its actor (who \
                 triggered the event) is a member. Its workflow comes from the base branch of the pull request and \
                 gets the secrets of the repository: a step that runs the code of the pull request runs it with \
                 those secrets"
                    .into(),
            );
        }
        out
    }
}

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
    cfg.organizations.iter().chain(&cfg.users).any(|o| same(&o.login, &t.owner) && same_id(t.owner_id, o.id))
}

fn listed_public(t: &TrustInputs, cfg: &TierConfig) -> bool {
    cfg.trusted_public.iter().any(|r| same(&r.owner, &t.owner) && same(&r.repo, &t.repo) && same_id(t.repo_id, r.id))
}

/// The rule of trust, a seam that can be swapped (DR-3). A rule computes
/// the tier of a job from its facts and the trust lists; the intake check
/// is the same for each rule. `cases::conformance` checks a rule against
/// the published case table.
pub trait TrustRule: Send + Sync {
    /// The tier of a job, or the refusal (`event-not-allowed`).
    fn compute(&self, t: &TrustInputs, cfg: &TierConfig) -> Result<Tier, RefusalCode>;

    /// The tier that a job of a set with the tier `set` runs under, or the
    /// refusal. A set marked `local` runs each job at `local`, with no rule.
    fn intake(&self, t: &TrustInputs, cfg: &TierConfig, set: Tier) -> Result<Tier, RefusalCode> {
        // A local set serves a source of the operator, never a forge: the
        // configuration refuses a forge source on a local set (call C14).
        if set == Tier::Local {
            return Ok(Tier::Local);
        }
        let computed = self.compute(t, cfg)?;
        if rank(computed) < rank(set) {
            return Err(RefusalCode::TierMismatch);
        }
        // A job more trusted than its set runs under the stricter tier.
        Ok(set)
    }
}

/// The rule of this module: the rule of the design (SPEC §6.3) with the
/// extra events of the owner (Q5, Q12) and the agent rule (T-106).
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultRule;

impl TrustRule for DefaultRule {
    fn compute(&self, t: &TrustInputs, cfg: &TierConfig) -> Result<Tier, RefusalCode> {
        let visible = match t.repo_visibility {
            Visibility::Private | Visibility::Internal => true,
            Visibility::Public => listed_public(t, cfg),
            Visibility::Unknown => false,
        };
        let allowed_event = TRUSTED_EVENTS.contains(&t.event)
            || (EXTRA_EVENTS.contains(&t.event) && cfg.extra_events.contains(&t.event));
        // For a pull request, the head is the repository of its branch:
        // `head_is_fork` is yes when that is not this repository.
        let trusted = visible
            && configured_owner(t, cfg)
            && t.head_is_fork == Fact::No
            && t.actor_class == ActorClass::Member
            && allowed_event;
        if trusted {
            return Ok(Tier::Trusted);
        }
        // A change that an agent made runs as untrusted code, in a guest,
        // also in the repository of a configured owner (T-106). Only with a
        // head of this repository: an extra event from a fork holds the
        // secrets, and a guest must not get them. A bot is never trusted:
        // it falls through to the rules below.
        if t.actor_class == ActorClass::Agent
            && visible
            && configured_owner(t, cfg)
            && t.head_is_fork == Fact::No
            && allowed_event
        {
            return Ok(Tier::Untrusted);
        }
        // Only a fork `pull_request` goes to a guest: GitHub gives it no
        // secret. A fork `pull_request_target` holds the secrets.
        if t.event == Event::PullRequest && t.head_is_fork == Fact::Yes {
            return Ok(Tier::Untrusted);
        }
        Err(RefusalCode::EventNotAllowed)
    }
}

/// The tier of a job by [`DefaultRule`], or `event-not-allowed`.
pub fn compute(t: &TrustInputs, cfg: &TierConfig) -> Result<Tier, RefusalCode> {
    DefaultRule.compute(t, cfg)
}

/// The order of trust: `untrusted` < `trusted` < `local`.
fn rank(t: Tier) -> u8 {
    match t {
        Tier::Untrusted => 0,
        Tier::Trusted => 1,
        Tier::Local => 2,
    }
}

/// The intake of a job by [`DefaultRule`].
pub fn intake(t: &TrustInputs, cfg: &TierConfig, set: Tier) -> Result<Tier, RefusalCode> {
    DefaultRule.intake(t, cfg, set)
}

pub mod cases;

#[cfg(test)]
mod tests;
