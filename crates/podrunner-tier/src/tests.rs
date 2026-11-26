//! The case table of `tests/cases.json`, an exhaustive check that no input
//! with an `unknown` field is trusted, and three planted defects that the
//! table finds.

use podrunner_schema::offer::{ActorClass, Event, TrustInputs, Visibility};
use podrunner_schema::{Fact, RefusalCode, Tier};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{EXTRA_EVENTS, Organization, TierConfig, compute, intake};

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
    /// The trust lists of this row, in place of those of the table.
    #[serde(default)]
    trust: Option<TierConfig>,
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
            let got = show(rule(&inputs, c.trust.as_ref().unwrap_or(&t.config), c.set));
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
            for actor_class in [A::Member, A::Outside, A::Agent, A::Bot, A::Unknown] {
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
                            forge: None,
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
    assert_eq!(inputs.len(), 12 * 3 * 5 * 4 * 3);
    // Also with each extra event listed (Q5, Q12).
    let mut every = cfg.clone();
    every.extra_events = EXTRA_EVENTS.to_vec();
    for t in inputs.iter().filter(|t| has_unknown(t)) {
        assert_ne!(compute(t, &cfg), Ok(Tier::Trusted), "{t:?}");
        assert_ne!(compute(t, &every), Ok(Tier::Trusted), "{t:?}, each extra event listed");
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

/// The rulings Q5 and Q12 of the owner: the operator may add only the events of
/// `EXTRA_EVENTS`, each with the facts of a trusted job; `other` and
/// `unknown` stay refused (T-102, T-518).
#[test]
fn extra_events_are_trusted_only_when_listed_and_only_those_five() {
    let cfg = |extra: Vec<Event>| TierConfig {
        organizations: vec![Organization { login: "o".into(), id: None }],
        users: vec![],
        trusted_public: vec![],
        extra_events: extra,
    };
    let job = |event: Event, fork: Fact| {
        let mut t: TrustInputs = serde_json::from_value(Value::Object(table().base)).unwrap();
        (t.event, t.head_is_fork, t.actor_class, t.repo_visibility) =
            (event, fork, ActorClass::Member, Visibility::Private);
        (t.owner, t.repo, t.owner_id, t.repo_id) = ("o".into(), "r".into(), None, None);
        t
    };
    for e in EXTRA_EVENTS {
        assert_eq!(compute(&job(e, Fact::No), &cfg(vec![])), Err(RefusalCode::EventNotAllowed), "{e:?} unlisted");
        assert_eq!(compute(&job(e, Fact::No), &cfg(vec![e])), Ok(Tier::Trusted), "{e:?} listed");
        assert!(compute(&job(e, Fact::Yes), &cfg(vec![e])).is_err(), "{e:?} from a fork");
    }
    assert_eq!(EXTRA_EVENTS.len(), 5);
    for e in [Event::Other, Event::Unknown] {
        assert!(cfg(vec![e]).check().is_err(), "{e:?} can never be listed");
        assert!(compute(&job(e, Fact::No), &cfg(vec![e])).is_err(), "{e:?}");
    }
    assert!(cfg(EXTRA_EVENTS.to_vec()).check().is_ok());
}

/// T-106: an agent's job of an allowed event is untrusted (a guest), a
/// bot's job is never trusted, and a member's job is trusted as before.
#[test]
fn an_agent_needs_a_guest_and_a_bot_is_never_trusted() {
    let cfg = TierConfig {
        organizations: vec![Organization { login: "o".into(), id: None }],
        users: vec![],
        trusted_public: vec![],
        extra_events: vec![],
    };
    let job = |actor: ActorClass, event: Event| {
        let mut t: TrustInputs = serde_json::from_value(Value::Object(table().base)).unwrap();
        (t.event, t.head_is_fork, t.actor_class, t.repo_visibility) = (event, Fact::No, actor, Visibility::Private);
        (t.owner, t.repo, t.owner_id, t.repo_id) = ("o".into(), "r".into(), None, None);
        t
    };
    assert_eq!(compute(&job(ActorClass::Member, Event::Push), &cfg), Ok(Tier::Trusted));
    assert_eq!(compute(&job(ActorClass::Agent, Event::Push), &cfg), Ok(Tier::Untrusted));
    assert_eq!(compute(&job(ActorClass::Agent, Event::Other), &cfg), Err(RefusalCode::EventNotAllowed));
    assert_eq!(compute(&job(ActorClass::Bot, Event::Push), &cfg), Err(RefusalCode::EventNotAllowed));
    // An agent's job never runs on a trusted set.
    assert_eq!(intake(&job(ActorClass::Agent, Event::Push), &cfg, Tier::Trusted), Err(RefusalCode::TierMismatch));
    // An agent's job from a fork or an unknown head never goes to a guest,
    // with each extra event listed: the job of an extra event holds the
    // secrets. Only a fork `pull_request` (no secrets) does, as for each
    // actor.
    let mut every = table().config;
    every.extra_events = EXTRA_EVENTS.to_vec();
    for t in each_input().iter().filter(|t| t.actor_class == ActorClass::Agent && t.head_is_fork != Fact::No) {
        if compute(t, &every) == Ok(Tier::Untrusted) {
            assert!(t.event == Event::PullRequest && t.head_is_fork == Fact::Yes, "{t:?}");
        }
    }
}

/// T-191: a repository of a listed user is trusted as one of a listed
/// organization; an owner on neither list is not.
#[test]
fn a_listed_user_owns_a_trusted_repository() {
    let job = |owner: &str| {
        let mut t: TrustInputs = serde_json::from_value(Value::Object(table().base)).unwrap();
        (t.event, t.head_is_fork, t.actor_class, t.repo_visibility) =
            (Event::Push, Fact::No, ActorClass::Member, Visibility::Private);
        (t.owner, t.repo, t.owner_id, t.repo_id) = (owner.into(), "r".into(), None, None);
        t
    };
    let cfg: TierConfig = serde_json::from_value(serde_json::json!({ "users": [{ "login": "alice" }] })).unwrap();
    assert_eq!(compute(&job("alice"), &cfg), Ok(Tier::Trusted));
    assert_eq!(compute(&job("mallory"), &cfg), Err(RefusalCode::EventNotAllowed));
}

/// The owner, Q12: with `pull_request_target` in `extra_events`, a job of
/// that event is trusted with the facts of each trusted job, and only then.
/// A fork never goes to the untrusted tier: the job holds the secrets.
#[test]
fn pull_request_target_is_trusted_only_with_the_opt_in_and_the_facts() {
    let cfg = |extra: Vec<Event>| TierConfig {
        organizations: vec![Organization { login: "o".into(), id: Some(7) }],
        users: vec![],
        trusted_public: vec![],
        extra_events: extra,
    };
    let (off, on) = (cfg(vec![]), cfg(vec![Event::PullRequestTarget]));
    assert!(on.check().is_ok(), "{:?}", on.check());
    let job = |fork: Fact, actor: ActorClass, owner: &str, vis: Visibility| {
        let mut t: TrustInputs = serde_json::from_value(Value::Object(table().base)).unwrap();
        (t.event, t.head_is_fork, t.actor_class, t.repo_visibility) = (Event::PullRequestTarget, fork, actor, vis);
        (t.owner, t.repo, t.owner_id, t.repo_id) = (owner.into(), "r".into(), None, None);
        t
    };
    let good = job(Fact::No, ActorClass::Member, "o", Visibility::Private);
    assert_eq!(compute(&good, &off), Err(RefusalCode::EventNotAllowed), "no opt-in");
    assert_eq!(compute(&good, &on), Ok(Tier::Trusted), "the opt-in");
    assert_eq!(intake(&good, &on, Tier::Trusted), Ok(Tier::Trusted));
    assert_eq!(intake(&good, &on, Tier::Untrusted), Ok(Tier::Untrusted), "the stricter set");
    let mut other_id = good.clone();
    other_id.owner_id = Some(8);
    for (why, t) in [
        ("a fork", job(Fact::Yes, ActorClass::Member, "o", Visibility::Private)),
        ("a fork that is not known", job(Fact::Unknown, ActorClass::Member, "o", Visibility::Private)),
        ("an outsider", job(Fact::No, ActorClass::Outside, "o", Visibility::Private)),
        ("an actor that is not known", job(Fact::No, ActorClass::Unknown, "o", Visibility::Private)),
        ("a bot", job(Fact::No, ActorClass::Bot, "o", Visibility::Private)),
        ("an owner that is not configured", job(Fact::No, ActorClass::Member, "mallory", Visibility::Private)),
        ("another owner id", other_id),
        ("a public repository that is not listed", job(Fact::No, ActorClass::Member, "o", Visibility::Public)),
        ("a fork of an outsider", job(Fact::Yes, ActorClass::Outside, "o", Visibility::Public)),
    ] {
        assert_eq!(compute(&t, &on), Err(RefusalCode::EventNotAllowed), "{why}");
        assert_eq!(intake(&t, &on, Tier::Untrusted), Err(RefusalCode::EventNotAllowed), "{why}, untrusted set");
    }
    // An agent's job runs in a guest, as for each event of the list (T-106).
    let agent = job(Fact::No, ActorClass::Agent, "o", Visibility::Private);
    assert_eq!(compute(&agent, &on), Ok(Tier::Untrusted));
    assert_eq!(compute(&agent, &off), Err(RefusalCode::EventNotAllowed));
    // `other` and `unknown` stay refused, also beside the opt-in.
    for e in [Event::Other, Event::Unknown] {
        assert!(cfg(vec![Event::PullRequestTarget, e]).check().is_err(), "{e:?}");
    }
}

/// The opt-in of an event changes the jobs of that event only, and makes
/// no job trusted that misses a fact of a trusted job.
#[test]
fn an_extra_event_changes_the_jobs_of_that_event_only() {
    let off = table().config;
    for e in EXTRA_EVENTS {
        let mut on = off.clone();
        on.extra_events = vec![e];
        assert!(on.check().is_ok(), "{e:?}");
        for t in each_input() {
            let (a, b) = (compute(&t, &off), compute(&t, &on));
            if t.event != e {
                assert_eq!(a, b, "{e:?} changed {t:?}");
            }
            if b == Ok(Tier::Trusted) && a != b {
                assert!(
                    t.head_is_fork == Fact::No && t.actor_class == ActorClass::Member && t.owner == "example-org",
                    "{t:?}"
                );
            }
        }
    }
    assert!(EXTRA_EVENTS.contains(&Event::PullRequestTarget));
}

const PUBLISHED: &str = "tier-cases.json";

fn published_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(PUBLISHED)
}

fn published_text() -> String {
    crate::cases::published(include_str!("../tests/cases.json")).unwrap()
}

/// T-518: `tier-cases.json` is the hand-written table made whole, with the
/// event lists of this code. `PODRUNNER_WRITE_SCHEMAS=1` writes it again.
#[test]
fn the_published_case_table_is_in_step_with_the_code() {
    let text = published_text();
    if std::env::var_os("PODRUNNER_WRITE_SCHEMAS").is_some() {
        std::fs::write(published_path(), &text).unwrap();
    }
    let file = std::fs::read_to_string(published_path()).unwrap_or_default();
    assert!(
        file == text,
        "{} differs from the code: run PODRUNNER_WRITE_SCHEMAS=1 cargo test -p podrunner-tier",
        PUBLISHED
    );
    assert_eq!(crate::cases::count(&file), Ok(table().cases.len()));
    let v: Value = serde_json::from_str(&file).unwrap();
    assert_eq!(v["rule"]["extra_events"][4], "pull_request_target");
}

/// Each event of the offer has a row by its name, with the opt-in off and
/// on (G-13).
#[test]
fn the_published_table_names_each_event_with_the_opt_in_off_and_on() {
    let v: Value = serde_json::from_str(&published_text()).unwrap();
    let rows = v["cases"].as_array().unwrap();
    for e in each_input().iter().map(|t| t.event) {
        let name = crate::event_name(e);
        let with = |on: bool| {
            rows.iter().any(|r| {
                r["trust_inputs"]["event"] == name.as_str()
                    && r["config"]["extra_events"].as_array().unwrap().iter().any(|x| x == name.as_str()) == on
            })
        };
        assert!(with(false), "{name}: no row with the opt-in off");
        if EXTRA_EVENTS.contains(&e) {
            assert!(with(true), "{name}: no row with the opt-in on");
        }
    }
}

/// The default rule passes the published table through the harness of the
/// seam; a swapped rule with a planted defect fails it.
#[test]
fn the_harness_passes_the_default_rule_and_finds_each_planted_rule() {
    use crate::{DefaultRule, TrustRule};
    // The text of the code: the test above holds the file to it.
    let file = published_text();
    assert_eq!(crate::cases::conformance(&DefaultRule, &file), Ok(vec![]));
    /// A rule that changes the facts of a job before the default rule.
    struct Plant(fn(&mut TrustInputs, &mut TierConfig));
    impl TrustRule for Plant {
        fn compute(&self, t: &TrustInputs, cfg: &TierConfig) -> Result<Tier, RefusalCode> {
            let (mut t, mut cfg) = (t.clone(), cfg.clone());
            (self.0)(&mut t, &mut cfg);
            DefaultRule.compute(&t, &cfg)
        }
    }
    let plants: [(&str, Plant); 6] = [
        ("pull_request_target with no opt-in", Plant(|_, c| c.extra_events.push(Event::PullRequestTarget))),
        (
            "the opt-in reads a fork as no fork",
            Plant(|t, _| {
                if t.event == Event::PullRequestTarget {
                    t.head_is_fork = Fact::No;
                }
            }),
        ),
        (
            "the opt-in reads an outsider as a member",
            Plant(|t, _| {
                if t.event == Event::PullRequestTarget {
                    t.actor_class = ActorClass::Member;
                }
            }),
        ),
        (
            "a fork pull_request_target goes to a guest",
            Plant(|t, _| {
                if t.event == Event::PullRequestTarget && t.head_is_fork == Fact::Yes {
                    t.event = Event::PullRequest;
                }
            }),
        ),
        (
            "an agent's job from a fork or an unknown head reads as no fork",
            Plant(|t, _| {
                if t.actor_class == ActorClass::Agent {
                    t.head_is_fork = Fact::No;
                }
            }),
        ),
        (
            "other and unknown read as a listed event",
            Plant(|t, _| {
                if matches!(t.event, Event::Other | Event::Unknown) {
                    t.event = Event::PullRequestTarget;
                }
            }),
        ),
    ];
    for (name, plant) in plants {
        let wrong = crate::cases::conformance(&plant, &file).unwrap();
        assert!(!wrong.is_empty(), "the harness passes the plant: {name}");
    }
    // A table of another schema is refused, not passed.
    assert!(crate::cases::conformance(&DefaultRule, &file.replacen("\"schema\": 1", "\"schema\": 2", 1)).is_err());
}

/// T-642: a wrong value in `extra_events` lists the events that the key
/// accepts, not each variant of the event type; the names agree with
/// `EXTRA_EVENTS`.
#[test]
fn a_wrong_extra_event_lists_the_accepted_events() {
    let names: Vec<String> = EXTRA_EVENTS.iter().map(|e| crate::event_name(*e)).collect();
    assert_eq!(names, crate::EXTRA_NAMES);
    let err = serde_json::from_value::<TierConfig>(serde_json::json!({ "extra_events": ["release", "bogus"] }))
        .unwrap_err()
        .to_string();
    assert!(err.contains("`bogus`") && err.contains("pull_request_target"), "{err}");
    assert!(!err.contains("unknown,") && !err.contains("other,"), "{err}");
    // A name of an event that the key refuses still reads: `check` words it.
    for e in ["other", "unknown", "push"] {
        let c: TierConfig = serde_json::from_value(serde_json::json!({ "extra_events": [e] })).unwrap();
        assert_eq!(c.extra_events.len(), 1, "{e}");
    }
}

/// T-642: a repeated event and an event that is trusted with no listing
/// give one line each; a clean list gives none.
#[test]
fn a_repeated_or_trusted_extra_event_gives_one_line() {
    let cfg = |extra: &[Event]| TierConfig { extra_events: extra.to_vec(), ..TierConfig::default() };
    assert!(cfg(&[Event::Release, Event::WorkflowRun]).notes().is_empty());
    let twice = cfg(&[Event::Release, Event::Release, Event::Release]).notes();
    assert_eq!(twice.len(), 2, "{twice:?}");
    assert!(twice.iter().all(|n| n.contains("release") && n.contains("twice")), "{twice:?}");
    let trusted = cfg(&[Event::Push]);
    assert!(trusted.check().is_ok());
    let notes = trusted.notes();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("push") && notes[0].contains("trusted with no listing"), "{notes:?}");
    // The line of the opt-in cites the residual R8.
    let r8 = cfg(&[Event::PullRequestTarget]).notes();
    assert!(r8.len() == 1 && r8[0].contains("R8") && !r8[0].contains("Q12"), "{r8:?}");
}
