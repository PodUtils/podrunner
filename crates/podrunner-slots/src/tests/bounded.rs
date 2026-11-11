//! T-068: a long `serve` stays bounded: a final job leaves memory an hour
//! after its end, its suppression an hour later, the reports keep the last
//! 256, and a restart compacts the journal to what a later start needs.

use std::time::Duration;

use podrunner_schema::{Boundary, EgressHold, Principal};
use serde_json::json;

use super::*;
use crate::forget::{REPORTS_KEPT, keep};

#[test]
fn bounded_jobs_suppressions_and_journal() {
    let jobs = 40;
    let lines: Vec<String> = (0..jobs).map(|n| offer(0, &format!("j-{n}"), json!({}))).collect();
    let kinds = || {
        vec![KindSpec {
            name: "fake",
            slots: 8,
            capability: capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory),
            command: sh("exit 0"),
        }]
    };
    let mut bed = Bed::new(config(""), vec![("a", "trusted", None, lines)], kinds());
    bed.until(600, |s| s.jobs().count() == jobs && Bed::all_final(s));
    assert_eq!(bed.slots.reports.len(), jobs);
    let before = std::fs::read_to_string(bed.dir.0.join("state/journal.jsonl")).unwrap().lines().count();
    assert!(before > jobs, "{before} lines");

    // An hour after their end, the final jobs leave memory; their ids stay
    // suppressed for one more hour, then go too.
    bed.clock.advance(Duration::from_secs(3601));
    bed.tick();
    assert_eq!(bed.slots.jobs().count(), 0);
    assert_eq!(bed.slots.suppressed.len(), jobs);
    bed.clock.advance(Duration::from_secs(3601));
    bed.tick();
    assert!(bed.slots.suppressed.is_empty());

    // A restart writes the journal again: no job is open, so one line keeps
    // the highest unit number, and the unit ids go on from it.
    let Bed { slots, clock, dir } = bed;
    drop(slots);
    let bed = Bed::in_dir(config(""), vec![("a", "trusted", None, vec![])], kinds(), dir, clock);
    let after = std::fs::read_to_string(bed.dir.0.join("state/journal.jsonl")).unwrap();
    assert_eq!(after.lines().count(), 1, "{after}");
    // Unit ids start at u-0; the kept line is final, so the job stays closed.
    assert!(after.contains(&format!("\"u-{}\"", jobs - 1)), "the highest unit: {after}");
    assert!(after.contains("\"reported\""), "{after}");
    assert!(bed.slots.reports.is_empty(), "the restart reported a job again");
    assert!(!bed.dir.0.join("state/journal.jsonl.part").exists());
}

#[test]
fn bounded_reports_keep_the_last() {
    let mut reports = std::collections::VecDeque::new();
    let r = |n: usize| podrunner_schema::RunReport {
        schema: 1,
        job_id: podrunner_schema::JobId::new(&format!("j-{n}")).unwrap(),
        source: "a".into(),
        tier: None,
        host: "h".into(),
        exit_reason: podrunner_schema::ExitReason::Success,
        exit_code: Some(0),
        wall_s: 0,
        refusal_code: None,
        unit: None,
    };
    for n in 0..REPORTS_KEPT + 10 {
        keep(&mut reports, r(n));
    }
    assert_eq!(reports.len(), REPORTS_KEPT);
    assert_eq!(reports.front().unwrap().job_id.as_str(), "j-10");
}
