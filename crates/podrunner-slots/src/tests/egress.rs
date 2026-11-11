//! The egress module behind slots: a listener for each unit and its close;
//! no `trusted` job on a kind with no hold; when egress ends, no offer and
//! each running unit ends (fail closed).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use podrunner_schema::{Boundary, EgressHold, ExitReason, Principal, RefusalCode, Tier};
use serde_json::json;

use super::*;
use crate::EgressPort;
use crate::placement::{Kind, place};

#[derive(Clone, Default)]
struct Seen {
    opened: Arc<Mutex<Vec<(String, String)>>>,
    closed: Arc<Mutex<Vec<String>>>,
    dead: Arc<AtomicBool>,
}

impl EgressPort for Seen {
    fn open(&self, unit: &str, policy: &str, _socket: &std::path::Path) -> Result<usize, String> {
        self.opened.lock().unwrap().push((unit.into(), policy.into()));
        Ok(usize::from(policy.contains("github.io")))
    }
    fn close(&self, unit: &str) {
        self.closed.lock().unwrap().push(unit.into());
    }
    fn alive(&self) -> bool {
        !self.dead.load(Ordering::SeqCst)
    }
}

fn confined() -> podrunner_schema::Capability {
    capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory)
}

#[test]
fn each_unit_gets_a_listener_and_the_unit_sees_its_way_out() {
    let seen = Seen::default();
    let policy = json!({ "policy": { "schema": 1, "mode": "allow", "allow": [{ "host": "github.com" }] } });
    let bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", policy)])],
        vec![KindSpec { name: "fake", slots: 1, capability: confined(), command: sh("test -n \"$PODRUNNER_EGRESS\"") }],
    );
    let mut bed = Bed { slots: bed.slots.with_egress(Box::new(seen.clone())), ..bed };
    bed.until(30, |s| s.jobs().count() == 1 && Bed::all_final(s));
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success, "the unit saw PODRUNNER_EGRESS");
    let opened = seen.opened.lock().unwrap().clone();
    assert_eq!(opened.len(), 1);
    assert!(opened[0].1.contains("github.com"), "the policy of the offer: {}", opened[0].1);
    assert_eq!(*seen.closed.lock().unwrap(), [opened[0].0.clone()]);
    assert!(!bed.slots.audit_lines().iter().any(|l| l.audit == crate::audit::Kind::EgressFlagged));
}

/// T-048: a policy that allows a host under an open suffix is an audit
/// line, with the count of the rules and no rule.
#[test]
fn a_flagged_allow_rule_is_an_audit_line() {
    let seen = Seen::default();
    let policy = json!({ "policy": { "schema": 1, "mode": "allow", "allow": [{ "host": "*.github.io" }] } });
    let bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", policy)])],
        vec![KindSpec { name: "fake", slots: 1, capability: confined(), command: sh("exit 0") }],
    );
    let mut bed = Bed { slots: bed.slots.with_egress(Box::new(seen)), ..bed };
    bed.until(30, |s| s.jobs().count() == 1 && Bed::all_final(s));
    let lines: Vec<_> =
        bed.slots.audit_lines().iter().filter(|l| l.audit == crate::audit::Kind::EgressFlagged).collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].detail.as_deref(), Some("1 rules"));
}

#[test]
fn a_kind_with_no_hold_gets_no_trusted_job() {
    let none = capability(Boundary::Confined, Principal::Distinct, EgressHold::None);
    let k = Kind { capability: none, free: 1, running: 0, repos: vec![] };
    let line: Value = serde_json::from_str(&offer(0, "j-1", json!({}))).unwrap();
    let o = serde_json::from_value(line["offer"].clone()).unwrap();
    assert_eq!(place(&o, Tier::Trusted, &[k], &config(""), NOW_UNIX), Err(RefusalCode::HoldNotAvailable));
}

#[test]
fn when_egress_ends_no_offer_comes_and_each_unit_ends() {
    let seen = Seen::default();
    let lines = vec![offer(0, "long", json!({})), offer(20, "later", json!({}))];
    let bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, lines)],
        vec![KindSpec { name: "fake", slots: 2, capability: confined(), command: sh("sleep 300") }],
    );
    let mut bed = Bed { slots: bed.slots.with_egress(Box::new(seen.clone())), ..bed };
    bed.until(10, |s| s.jobs().any(|j| j.state == State::Started));
    seen.dead.store(true, Ordering::SeqCst);
    bed.until(60, |s| s.jobs().all(|j| j.state.is_final()));
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::UnitFailed);
    for _ in 0..30 {
        bed.tick();
    }
    assert_eq!(bed.state("later"), None, "no offer while egress is down");
}

#[test]
fn a_long_state_directory_gets_a_short_egress_directory() {
    let short = crate::run::egress_dir(std::path::Path::new("/var/lib/podrunner"));
    assert_eq!(short, std::path::Path::new("/var/lib/podrunner/egress"));
    let deep = std::path::PathBuf::from(format!("/{}", "d".repeat(150)));
    let other = crate::run::egress_dir(&deep);
    assert!(other.join("u-18446744073709551615.sock").as_os_str().len() < 108, "{}", other.display());
    assert_eq!(other, crate::run::egress_dir(&deep), "the same directory each time");
}
