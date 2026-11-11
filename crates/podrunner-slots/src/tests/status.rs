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
    assert_eq!(st.pins[0].age_s, 0);
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
