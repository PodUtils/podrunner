//! T-108: the report of a job that ran carries the use of its unit.

use podrunner_schema::{Boundary, EgressHold, Principal, Residual};
use serde_json::json;

use super::*;

#[test]
fn the_report_of_a_unit_that_spins_has_its_cpu_time_and_memory() {
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: cap,
            command: sh("i=0; while [ $i -lt 2000000 ]; do i=$((i+1)); done"),
        }],
    );
    // A real second for each tick: the CPU time is real.
    for _ in 0..120 {
        bed.tick();
        std::thread::sleep(std::time::Duration::from_millis(100));
        if bed.slots.jobs().count() == 1 && Bed::all_final(&bed.slots) {
            break;
        }
    }
    let r = &bed.slots.reports[0];
    assert!(matches!(r.cpu_s, Some(Residual::Count(n)) if n >= 1), "{r:?}");
    assert!(matches!(r.peak_rss_mib, Some(Residual::Count(n)) if n >= 1), "{r:?}");
}
