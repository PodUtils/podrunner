//! T-494 (githome's G-16): the trust lists of each source. A login of one
//! forge is not a login of another.

use podrunner_schema::{Boundary, Capability, EgressHold, JobOffer, Principal, RefusalCode, Tier};
use podrunner_tier::{Organization, TierConfig};
use serde_json::{Value, json};

use super::*;
use crate::config::Config;
use crate::placement::{Kind, place_for};

fn job(owner: &str) -> JobOffer {
    let line: Value = serde_json::from_str(&offer(0, "j-1", json!({ "trust_inputs": { "owner": owner } }))).unwrap();
    serde_json::from_value(line["offer"].clone()).unwrap()
}

fn users(logins: &[&str]) -> TierConfig {
    TierConfig {
        users: logins.iter().map(|l| Organization { login: (*l).into(), id: None }).collect(),
        ..Default::default()
    }
}

fn kinds() -> Vec<Kind> {
    let c: Capability = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    vec![Kind { capability: c, free: 1, running: 0, repos: vec![], root_disk: None, init: None, warm: vec![] }]
}

#[test]
fn a_login_of_one_source_is_not_trusted_on_another() {
    let cfg = config("");
    let githome = users(&["alice"]);
    let github = users(&["bob"]);
    assert_eq!(
        place_for(&job("alice"), Tier::Trusted, &kinds(), &cfg, &githome, NOW_UNIX).map(|p| p.tier),
        Ok(Tier::Trusted)
    );
    assert_eq!(
        place_for(&job("alice"), Tier::Trusted, &kinds(), &cfg, &github, NOW_UNIX).map(|p| p.tier),
        Err(RefusalCode::EventNotAllowed)
    );
    assert_eq!(
        place_for(&job("bob"), Tier::Trusted, &kinds(), &cfg, &github, NOW_UNIX).map(|p| p.tier),
        Ok(Tier::Trusted)
    );
    assert_eq!(
        place_for(&job("bob"), Tier::Trusted, &kinds(), &cfg, &githome, NOW_UNIX).map(|p| p.tier),
        Err(RefusalCode::EventNotAllowed)
    );
}

fn two_sources(own: &str) -> String {
    format!(
        r#"host = "h"
[trust]
users = [{{ login = "alice" }}]
[[source]]
name = "githome"
kind = "program"
program = "/opt/d/githome-source"
tier = "trusted"
arch = "x86_64"
{own}
[[source]]
name = "github"
kind = "program"
program = "/opt/d/other"
tier = "trusted"
arch = "x86_64"
[pin]
"#
    )
}

#[test]
fn a_global_list_shared_with_a_program_source_is_refused() {
    let e = Config::parse(&two_sources("")).unwrap_err();
    assert!(e.contains("[trust] holds for the sources githome, github"), "{e}");
    // Each source with a list of its own (the second takes the global one
    // alone): accepted, and each source reads its own.
    let c = Config::parse(&two_sources("trust = { users = [{ login = \"carol\" }] }")).unwrap();
    let any = job("x").trust_inputs;
    assert_eq!(c.trust_for(&c.source[0], &any).unwrap().users[0].login, "carol");
    assert_eq!(c.trust_for(&c.source[1], &any).unwrap().users[0].login, "alice");
    let bad = two_sources("trust = { extra_events = [\"other\"] }");
    assert!(Config::parse(&bad).unwrap_err().contains("[[source]] githome"));
}

/// T-518 (the owner, Q12): a trust table may list `pull_request_target`,
/// the global one, the one of a source and the one of a forge; `other` and
/// `unknown` stay refused in each. A job of the event is trusted only with
/// the list that names it.
#[test]
fn a_trust_table_may_list_pull_request_target() {
    let own = r#"trust = { users = [{ login = "carol" }], extra_events = ["pull_request_target"] }"#;
    let c = Config::parse(&two_sources(own)).unwrap();
    let forges =
        r#"trust = [{ forge = "github.com", users = [{ login = "carol" }], extra_events = ["pull_request_target"] }]"#;
    let by_forge = Config::parse(&two_sources(forges)).unwrap();
    // `check` names each table that opts in, and no other.
    let notes = c.trust_notes();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].starts_with("[[source]] githome trust: extra_events: pull_request_target"), "{notes:?}");
    let notes = by_forge.trust_notes();
    assert!(notes.len() == 1 && notes[0].contains("of the forge github.com"), "{notes:?}");
    assert!(
        Config::parse(&two_sources("trust = { users = [{ login = \"carol\" }] }")).unwrap().trust_notes().is_empty()
    );
    for bad in [
        r#"trust = { extra_events = ["pull_request_target", "unknown"] }"#,
        r#"trust = [{ forge = "github.com", extra_events = ["other"] }]"#,
    ] {
        assert!(Config::parse(&two_sources(bad)).unwrap_err().contains("can never be trusted"), "{bad}");
    }
    // The pins of `config`: the file of two sources has none.
    let pins = config("");
    let mut j = job("carol");
    j.trust_inputs.event = podrunner_schema::offer::Event::PullRequestTarget;
    let on = c.trust_for(&c.source[0], &j.trust_inputs).unwrap();
    assert_eq!(place_for(&j, Tier::Trusted, &kinds(), &pins, on, NOW_UNIX).map(|p| p.tier), Ok(Tier::Trusted));
    let off = users(&["carol"]);
    assert_eq!(
        place_for(&j, Tier::Trusted, &kinds(), &pins, &off, NOW_UNIX).map(|p| p.tier),
        Err(RefusalCode::EventNotAllowed)
    );
    j.trust_inputs.head_is_fork = podrunner_schema::Fact::Yes;
    assert_eq!(
        place_for(&j, Tier::Untrusted, &kinds(), &pins, on, NOW_UNIX).map(|p| p.tier),
        Err(RefusalCode::EventNotAllowed)
    );
}

/// T-494 step 2: one source carries two forges; an offer takes the list of
/// its `forge`; an offer with no forge, or a forge that the source does not
/// list, is refused.
#[test]
fn each_forge_of_a_source_has_its_own_list() {
    let lists = r#"trust = [{ forge = "GitHub.com", users = [{ login = "bob" }] }, { forge = "githome.example", users = [{ login = "alice" }] }]"#;
    let c = Config::parse(&two_sources(lists)).unwrap();
    let s = &c.source[0];
    let offer = |owner: &str, forge: Option<&str>| {
        let mut t = job(owner).trust_inputs;
        t.forge = forge.map(String::from);
        t
    };
    let alice_githome = offer("alice", Some("githome.example"));
    assert_eq!(c.trust_for(s, &alice_githome).unwrap().users[0].login, "alice");
    assert_eq!(c.trust_for(s, &offer("bob", Some("github.com"))).unwrap().users[0].login, "bob");
    assert_eq!(c.trust_for(s, &offer("alice", None)), Err(RefusalCode::TierMismatch));
    assert_eq!(c.trust_for(s, &offer("alice", Some("gitlab.example"))), Err(RefusalCode::TierMismatch));
    // The list of GitHub does not trust the login of githome.
    let github = c.trust_for(s, &offer("alice", Some("github.com"))).unwrap();
    let mut j = job("alice");
    j.trust_inputs.forge = Some("github.com".into());
    assert!(place_for(&j, Tier::Trusted, &kinds(), &c, github, NOW_UNIX).is_err());
    let twice = r#"trust = [{ forge = "a.example" }, { forge = "A.example" }]"#;
    assert!(Config::parse(&two_sources(twice)).unwrap_err().contains("named twice"));
}

/// An empty list of forges is not a table with no owner: `check` refuses it.
#[test]
fn an_empty_list_of_forges_is_refused() {
    let e = Config::parse(&two_sources("trust = []")).unwrap_err();
    assert!(e.contains("[[source]] githome") && e.contains("names no forge"), "{e}");
}
