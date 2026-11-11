//! `status.v1`: the schema of `schemas/` is generated from the type, and a
//! status of a running host names its sources, kinds and pins.

use serde_json::json;

use super::*;
use crate::status::Status;

#[test]
fn the_status_schema_is_generated_from_the_type() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/status.v1.json");
    let mut s = schemars::schema_for!(Status);
    s.insert("$id".into(), "https://github.com/PodUtils/podrunner/schemas/status.v1.json".into());
    let text = serde_json::to_string_pretty(&s).unwrap() + "\n";
    if std::env::var_os("PODRUNNER_WRITE_SCHEMAS").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    assert!(std::fs::read_to_string(&path).unwrap_or_default() == text, "{} differs from the type", path.display());
}

#[test]
fn the_status_names_the_sources_the_kinds_and_the_pins() {
    let cap = capability(
        podrunner_schema::Boundary::Confined,
        podrunner_schema::Principal::Distinct,
        podrunner_schema::EgressHold::Advisory,
    );
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({ "arch": "aarch64" }))])],
        vec![KindSpec { name: "fake", slots: 2, capability: cap, command: sh("exit 0") }],
    );
    bed.until(10, |s| s.jobs().count() == 1 && Bed::all_final(s));
    let st = bed.slots.status();
    assert!(!st.no_tier_served);
    assert_eq!(st.sources[0].refused.get("arch-mismatch"), Some(&1));
    assert_eq!((st.kinds[0].name.as_str(), st.kinds[0].slots, st.kinds[0].busy), ("fake", 2, 0));
    assert!(st.pins[0].age_s <= 10, "{}", st.pins[0].age_s);
    let v = serde_json::to_value(&st).unwrap();
    assert_eq!(v["schema"], 1);
}

#[test]
fn a_host_with_no_kind_at_a_floor_serves_no_tier() {
    let low = capability(
        podrunner_schema::Boundary::Redirect,
        podrunner_schema::Principal::Shared,
        podrunner_schema::EgressHold::Advisory,
    );
    let bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![])],
        vec![KindSpec { name: "fake", slots: 1, capability: low, command: sh("exit 0") }],
    );
    assert!(bed.slots.status().no_tier_served);
}

#[test]
fn the_status_counts_unknown_trust_and_ends_without_completion() {
    let cap = capability(
        podrunner_schema::Boundary::Guest,
        podrunner_schema::Principal::Distinct,
        podrunner_schema::EgressHold::Enforced,
    );
    let lines = vec![
        offer(0, "known", json!({})),
        offer(0, "unknown", json!({ "trust_inputs": { "actor_class": "unknown" } })),
        offer(0, "done", json!({})),
        r#"{"at_s":2,"completed":{"job_id":"done","result":"succeeded"}}"#.to_string(),
    ];
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, lines)],
        vec![KindSpec { name: "fake", slots: 3, capability: cap, command: sh("sleep 0.2") }],
    );
    bed.until(200, |s| s.jobs().count() == 3 && Bed::all_final(s));
    let st = bed.slots.status();
    assert_eq!(st.sources[0].trust_unknown, 1);
    assert_eq!(st.sources[0].refused.get("event-not-allowed"), Some(&1));
    // "known" ran with no completed event; "done" got one.
    assert_eq!(st.ended_without_completion, 1);
}

/// DR-4: a trusted job at `redirect` runs, and its report and the status
/// name the boundary, the principal and the hold (GitHub #47).
#[test]
fn a_job_at_redirect_names_the_boundary_and_the_hold() {
    use podrunner_schema::{Boundary, EgressHold, Principal};
    let cap = capability(Boundary::Redirect, Principal::Shared, EgressHold::Advisory);
    let mut bed = Bed::new(
        config_signed("repository = \"example-org/app\"\n[tier.trusted]\nmin_boundary = \"redirect\""),
        vec![("a", "trusted", Some(1), vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec { name: "fake", slots: 1, capability: cap, command: sh("exit 0") }],
    );
    bed.until(50, |s| s.jobs().count() == 1 && Bed::all_final(s));
    let unit = bed.slots.reports[0].unit.clone().expect("the unit fields");
    assert_eq!(
        (unit.boundary, unit.principal, unit.egress_hold),
        (Boundary::Redirect, Principal::Shared, EgressHold::Advisory)
    );
    let st = bed.slots.status();
    assert!(!st.no_tier_served);
    assert_eq!(st.kinds[0].probe.boundary, Boundary::Redirect);
    assert_eq!(st.kinds[0].probe.egress_hold, EgressHold::Advisory);
}

/// GitHub #30: a pin that passes its age limit between the claim and the
/// start is `image-stale` at the start; the runner's exit code 7 (its
/// version is too old) stops each new start until the operator acts.
#[test]
fn a_stale_pin_is_refused_at_the_start_and_exit_7_degrades() {
    use podrunner_schema::{Boundary, EgressHold, Principal, RefusalCode};
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let mut bed = Bed::new(
        config_pin("", "max_age_s = 1"),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec { name: "fake", slots: 1, capability: cap.clone(), command: sh("exit 0") }],
    );
    // The source gives the credential at the third call: two seconds after
    // the claim, when the pin is past its limit of one second.
    let empty = Box::new(podrunner_fake::FakeSource::new("x", Arc::new(ManualClock::default()), vec![]));
    let inner = std::mem::replace(&mut bed.slots.sources[0].source, empty);
    bed.slots.sources[0].source = Box::new(LateMint { inner, left: 2 });
    bed.until(20, |s| s.jobs().count() == 1 && Bed::all_final(s));
    assert_eq!(
        bed.slots.reports[0].refusal_code,
        Some(RefusalCode::ImageStale),
        "{:?} {:?}",
        bed.slots.reports,
        bed.slots.audit_lines()
    );
    assert!(bed.slots.degraded().is_none());

    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec { name: "fake", slots: 1, capability: cap, command: sh("exit 7") }],
    );
    bed.until(20, |s| s.jobs().count() == 1 && Bed::all_final(s));
    assert!(bed.slots.degraded().is_some_and(|d| d.contains("too old")), "{:?}", bed.slots.degraded());
}

/// A source whose first mints are `credential-unavailable`.
struct LateMint {
    inner: Box<dyn podrunner_source::JobSource>,
    left: u32,
}

impl podrunner_source::JobSource for LateMint {
    fn hello(&mut self, c: &str, v: &str) -> Result<podrunner_source::Hello, podrunner_source::SourceError> {
        self.inner.hello(c, v)
    }
    fn next(
        &mut self,
        n: u32,
        w: std::time::Duration,
    ) -> Result<Vec<podrunner_source::Event>, podrunner_source::SourceError> {
        self.inner.next(n, w)
    }
    fn ack(&mut self, j: &podrunner_schema::JobId) -> Result<(), podrunner_source::SourceError> {
        self.inner.ack(j)
    }
    fn claim(&mut self, j: &podrunner_schema::JobId, r: &str) -> Result<(), podrunner_source::SourceError> {
        self.inner.claim(j, r)
    }
    fn mint(
        &mut self,
        j: &podrunner_schema::JobId,
        r: &str,
    ) -> Result<podrunner_schema::Credential, podrunner_source::SourceError> {
        if self.left > 0 {
            self.left -= 1;
            return Err(podrunner_source::SourceError::new(
                podrunner_source::SourceCode::CredentialUnavailable,
                "later",
            ));
        }
        self.inner.mint(j, r)
    }
    fn report(
        &mut self,
        j: &podrunner_schema::JobId,
        r: &serde_json::Map<String, Value>,
    ) -> Result<(), podrunner_source::SourceError> {
        self.inner.report(j, r)
    }
    fn audit(&mut self, l: &serde_json::Map<String, Value>) -> Result<(), podrunner_source::SourceError> {
        self.inner.audit(l)
    }
    fn withdraw(&mut self, j: &podrunner_schema::JobId, r: &str) -> Result<(), podrunner_source::SourceError> {
        self.inner.withdraw(j, r)
    }
    fn release(&mut self, j: &podrunner_schema::JobId) -> Result<(), podrunner_source::SourceError> {
        self.inner.release(j)
    }
}

#[test]
fn the_status_shows_the_age_of_each_credential_and_of_the_key() {
    let cap = capability(
        podrunner_schema::Boundary::Guest,
        podrunner_schema::Principal::Distinct,
        podrunner_schema::EgressHold::Enforced,
    );
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "slow", json!({}))])],
        vec![KindSpec { name: "fake", slots: 1, capability: cap, command: sh("sleep 30") }],
    );
    bed.until(20, |s| s.jobs().any(|j| j.minted_ms.is_some()));
    for _ in 0..5 {
        bed.tick();
    }
    let st = bed.slots.status();
    assert_eq!(st.sources[0].credential_ages_s.len(), 1, "{:?}", st.sources[0]);
    assert!(st.sources[0].credential_ages_s[0] >= 5, "{:?}", st.sources[0].credential_ages_s);
    assert_eq!(st.sources[0].key_age_s, None);
    // The age of a key file comes from its time of change, not its bytes.
    let file = bed.dir.0.join("key.pem");
    std::fs::write(&file, "not a key").unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let age = crate::status::key_age_s(file.to_str().unwrap(), now + 60).unwrap();
    // A second can pass between the write and `now`.
    assert!((60..=61).contains(&age), "{age}");
    assert_eq!(crate::status::key_age_s("/nonexistent/key", now), None);
}
