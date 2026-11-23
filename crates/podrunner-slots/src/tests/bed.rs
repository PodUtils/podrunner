//! T-589: `Bed::until` gives a slow fake unit the real time that it needs,
//! and moves the manual clock by no more than its ticks.

use std::time::Duration;

use podrunner_schema::{Boundary, EgressHold, ExitReason, Principal};
use serde_json::json;

use super::*;

/// A unit that needs more real time than 20 ticks give, as on a loaded
/// host: it ends only after the 20th tick. The job ends with success, and
/// the clock moves by 20 s only.
#[test]
fn a_slow_unit_ends_after_the_ticks_of_its_clock() {
    let go = TempDir::new();
    let file = go.0.join("go");
    let wait = format!("while [ ! -e '{}' ]; do sleep 0.01; done", file.display());
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec { name: "fake", slots: 1, capability: cap, command: sh(&wait) }],
    );
    let (clock, start) = (bed.clock.clone(), bed.clock.wall());
    let at_20 = std::thread::spawn(move || {
        let end = std::time::Instant::now() + REAL;
        while clock.wall().duration_since(start).unwrap() < Duration::from_secs(20) && std::time::Instant::now() < end {
            std::thread::sleep(Duration::from_millis(1));
        }
        std::fs::write(file, "").unwrap();
    });
    let ticks = bed.until(20, |s| s.jobs().count() == 1 && Bed::all_final(s));
    at_20.join().unwrap();
    assert_eq!(ticks, 20);
    assert_eq!(bed.clock.wall().duration_since(start).unwrap(), Duration::from_secs(20));
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success, "{:?}", bed.slots.reports[0]);
}

/// A condition that never holds fails at the deadline of real time.
#[test]
#[should_panic(expected = "not done after 3 ticks and 50ms of real time")]
fn a_condition_that_never_holds_fails_at_its_deadline() {
    let mut bed = Bed::new(config(""), vec![("a", "trusted", None, vec![])], vec![]);
    bed.until_within(3, Duration::from_millis(50), |_| false);
}
