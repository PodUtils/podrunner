//! T-502 (githome's G-27): a cancel of the source destroys the unit in a
//! bounded time and reports `cancelled`, also for a payload that ignores
//! SIGTERM.

use podrunner_schema::{Boundary, EgressHold, ExitReason, Principal};
use serde_json::json;

use super::*;

#[test]
fn a_cancel_destroys_its_unit_within_60_s() {
    for (name, script) in
        [("ends at TERM", "exec sleep 300"), ("ignores TERM", "trap '' TERM; while :; do sleep 1; done")]
    {
        let mut bed = Bed::new(
            config(""),
            vec![("a", "trusted", None, vec![offer(0, "j-1", json!({})), r#"{"at_s":5,"cancel":"j-1"}"#.to_string()])],
            vec![KindSpec {
                name: "fake",
                slots: 1,
                capability: capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory),
                command: sh(script),
            }],
        );
        // The cancel comes 5 s after the start: the unit is final 60 s
        // after it at the latest (grace_s 30, then a kill and the destroy).
        let ticks = bed.until(120, |s| s.jobs().count() == 1 && Bed::all_final(s));
        assert!(ticks <= 5 + 60, "{name}: {ticks} s from the start to the end");
        let r = &bed.slots.reports[0];
        assert_eq!(r.exit_reason, ExitReason::Cancelled, "{name}: {r:?}");
        let unit = r.unit.as_ref().expect("the unit of the job");
        assert_eq!(unit.residual_processes, podrunner_schema::Residual::Count(0), "{name}: {r:?}");
    }
}
