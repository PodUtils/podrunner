//! The case table of `tests/cases.json`, an exhaustive check that no input
//! with an `unknown` field is trusted, and three planted defects that the
//! table finds.

use podrunner_schema::offer::{ActorClass, Event, TrustInputs, Visibility};
use podrunner_schema::{Fact, RefusalCode, Tier};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{TierConfig, compute, intake};

type Rule = fn(&TrustInputs, &TierConfig, Tier) -> Result<Tier, RefusalCode>;

#[derive(Deserialize)]
struct Table {
    config: TierConfig,
    base: Map<String, Value>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    set: Tier,
    with: Map<String, Value>,
    expect: String,
}

fn table() -> Table {
    serde_json::from_str(include_str!("../tests/cases.json")).expect("tests/cases.json")
}

fn show(r: Result<Tier, RefusalCode>) -> String {
    match r {
        Ok(t) => serde_json::to_value(t).unwrap().as_str().unwrap().to_string(),
        Err(c) => c.as_str().to_string(),
    }
}

/// The names of the rows that `rule` gets wrong.
fn wrong_rows(rule: Rule) -> Vec<String> {
    let t = table();
    t.cases
        .iter()
        .filter_map(|c| {
            let mut inputs = t.base.clone();
            inputs.extend(c.with.clone());
            let inputs: TrustInputs = serde_json::from_value(Value::Object(inputs)).expect(&c.name);
            let got = show(rule(&inputs, &t.config, c.set));
            (got != c.expect).then(|| format!("{}: {got}, not {}", c.name, c.expect))
        })
        .collect()
}

#[test]
fn each_row_of_the_case_table_passes() {
    let wrong = wrong_rows(intake);
    assert!(wrong.is_empty(), "{wrong:#?}");
    assert!(table().cases.len() >= 30);
}

/// Each value of each field that the rule reads.
fn each_input() -> Vec<TrustInputs> {
    use ActorClass as A;
    use Event as E;
    use Visibility as V;
    let events = [
        E::Push,
        E::PullRequest,
        E::PullRequestTarget,
        E::WorkflowDispatch,
        E::Schedule,
        E::MergeGroup,
        E::WorkflowRun,
        E::IssueComment,
        E::Release,
        E::RepositoryDispatch,
        E::Other,
        E::Unknown,
    ];
    let mut out = Vec::new();
    for event in events {
        for head_is_fork in [Fact::Yes, Fact::No, Fact::Unknown] {
            for actor_class in [A::Member, A::Outside, A::Unknown] {
                for repo_visibility in [V::Private, V::Internal, V::Public, V::Unknown] {
                    for (owner, repo) in [("example-org", "open-tool"), ("example-org", "app"), ("other", "app")] {
                        out.push(TrustInputs {
                            event,
                            head_is_fork,
                            actor_class,
                            repo_visibility,
                            owner: owner.into(),
                            repo: repo.into(),
                            run_id: 1,
                            git_ref: "refs/heads/main".into(),
                            workflow_ref: "w".into(),
                            owner_id: None,
                            repo_id: None,
                        });
                    }
                }
            }
        }
    }
    out
}

fn has_unknown(t: &TrustInputs) -> bool {
    t.event == Event::Unknown
        || t.head_is_fork == Fact::Unknown
        || t.actor_class == ActorClass::Unknown
        || t.repo_visibility == Visibility::Unknown
}

#[test]
fn no_input_with_an_unknown_field_is_trusted() {
    let cfg = table().config;
    let inputs = each_input();
    assert_eq!(inputs.len(), 12 * 3 * 3 * 4 * 3);
    for t in inputs.iter().filter(|t| has_unknown(t)) {
        assert_ne!(compute(t, &cfg), Ok(Tier::Trusted), "{t:?}");
    }
    // A control: some inputs are trusted, some untrusted.
    assert!(inputs.iter().any(|t| compute(t, &cfg) == Ok(Tier::Trusted)));
    assert!(inputs.iter().any(|t| compute(t, &cfg) == Ok(Tier::Untrusted)));
}

#[test]
fn local_is_never_computed() {
    let cfg = table().config;
    assert!(each_input().iter().all(|t| compute(t, &cfg) != Ok(Tier::Local)));
}

/// Plant: an `unknown` fork flag or actor reads as the good value.
fn unknown_is_good(t: &TrustInputs, cfg: &TierConfig, set: Tier) -> Result<Tier, RefusalCode> {
    let mut t = t.clone();
    if t.head_is_fork == Fact::Unknown {
        t.head_is_fork = Fact::No;
    }
    if t.actor_class == ActorClass::Unknown {
        t.actor_class = ActorClass::Member;
    }
    intake(&t, cfg, set)
}

/// Plant: no intake check; each job runs under the tier of its set.
fn no_intake_check(t: &TrustInputs, cfg: &TierConfig, set: Tier) -> Result<Tier, RefusalCode> {
    compute(t, cfg).map(|_| set)
}

/// Plant: `pull_request_target` from a member is trusted.
fn target_from_member(t: &TrustInputs, cfg: &TierConfig, set: Tier) -> Result<Tier, RefusalCode> {
    let mut t = t.clone();
    if t.event == Event::PullRequestTarget && t.actor_class == ActorClass::Member {
        t.event = Event::PullRequest;
    }
    intake(&t, cfg, set)
}

#[test]
fn the_case_table_finds_each_plant() {
    for (name, plant) in [
        ("unknown is good", unknown_is_good as Rule),
        ("no intake check", no_intake_check),
        ("pull_request_target from a member", target_from_member),
    ] {
        assert!(!wrong_rows(plant).is_empty(), "the case table passes the plant {name}");
    }
}
