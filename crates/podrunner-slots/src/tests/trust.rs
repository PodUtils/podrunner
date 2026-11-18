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
    vec![Kind { capability: c, free: 1, running: 0, repos: vec![] }]
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
    assert_eq!(c.trust_of(&c.source[0]).users[0].login, "carol");
    assert_eq!(c.trust_of(&c.source[1]).users[0].login, "alice");
    let bad = two_sources("trust = { extra_events = [\"pull_request_target\"] }");
    assert!(Config::parse(&bad).unwrap_err().contains("[[source]] githome"));
}
