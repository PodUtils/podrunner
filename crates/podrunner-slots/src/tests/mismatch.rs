//! T-642: the audit line of a `job-mismatch` names the runner names.

use podrunner_schema::{Boundary, EgressHold, Principal};
use serde_json::json;

use super::*;

fn mismatch_line(runner: &str) -> String {
    let lines = vec![
        offer(0, "j-1", json!({})),
        format!(r#"{{"at_s":3,"started":{{"job_id":"j-1","runner_name":"{runner}"}}}}"#),
    ];
    let capability = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, lines)],
        vec![KindSpec { name: "fake", slots: 1, capability, command: sh("sleep 300") }],
    );
    bed.until(40, |s| s.jobs().count() == 1 && Bed::all_final(s));
    let audit = bed.slots.audit_lines();
    let line = audit.iter().find(|a| a.audit == crate::audit::Kind::JobMismatch).expect("a job-mismatch line");
    line.detail.clone().expect("a detail")
}

#[test]
fn a_foreign_runner_name_is_in_the_audit_line_with_the_name_of_the_unit() {
    let detail = mismatch_line("someone-else");
    assert!(detail.contains("someone-else") && detail.contains("podrunner-host-a-u-"), "{detail}");
}

#[test]
fn a_long_runner_name_is_cut_in_the_audit_line() {
    let detail = mismatch_line(&"x".repeat(500));
    assert!(detail.len() < 300 && detail.contains(&"x".repeat(64)), "{detail}");
}
