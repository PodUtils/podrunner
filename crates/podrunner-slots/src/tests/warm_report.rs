//! T-657 (githome's G-25): the run report says how the unit of a job
//! started (`unit.start`, `unit.warm_s`, `unit.boot_s`), and the status
//! shows the warm units of each kind.

use podrunner_schema::{UnitStart, WarmStatus};
use serde_json::json;

use super::warm::{confined, ready, warm_bed};
use super::*;
use crate::config::WarmCount;

/// The report of one job of the offer at `at_s` on a fake kind with
/// `warm` warm units, whose command exits 0.
fn report_of(warm: u32, at_s: u64) -> podrunner_schema::RunReport {
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(at_s, "j-1", json!({}))])],
        vec![KindSpec { name: "fake", slots: 2, capability: confined(), command: sh("exit 0") }],
    );
    if warm > 0 {
        bed.slots.kinds[0].cfg.warm = Some(WarmCount::Count(warm));
    }
    bed.until(120, |s| s.jobs().count() == 1 && Bed::all_final(s));
    bed.slots.reports[0].clone()
}

#[test]
fn warm_report_a_warm_job_says_warm_with_its_wait_and_its_boot() {
    let r = report_of(1, 8);
    let u = r.unit.as_ref().expect("a unit ran");
    assert_eq!(u.start, Some(UnitStart::Warm), "{r:?}");
    // The warm unit is ready after a tick or two, and waits for the offer
    // at 8 s of the manual clock.
    assert!(u.warm_s.is_some_and(|s| (1..=8).contains(&s)), "{:?}", u.warm_s);
    assert!(u.boot_s.is_some_and(|s| s <= 7), "{:?}", u.boot_s);
    assert!(!r.unknown_reasons.contains_key("warm_s") && !r.unknown_reasons.contains_key("boot_s"), "{r:?}");
    r.check().unwrap();
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["unit"]["start"], "warm", "{v}");
}

#[test]
fn warm_report_a_cold_job_says_cold_and_why_it_has_no_unit_times() {
    let r = report_of(0, 0);
    let u = r.unit.as_ref().expect("a unit ran");
    assert_eq!((u.start, u.warm_s, u.boot_s), (Some(UnitStart::Cold), None, None), "{r:?}");
    assert!(r.unknown_reasons.get("warm_s").is_some_and(|w| w.contains("cold")), "{r:?}");
    assert!(r.unknown_reasons.get("boot_s").is_some_and(|w| w.contains("ready mark")), "{r:?}");
    r.check().unwrap();
}

#[test]
fn warm_report_the_status_shows_the_warm_units_of_each_kind() {
    let mut bed = warm_bed(3, 2, vec![], "");
    let st = bed.slots.status();
    let want = |booting, ready| Some(WarmStatus { target: 2, booting, ready, paused_reason: None });
    assert_eq!((st.kinds[0].warm.clone(), st.warm_units), (want(0, 0), 0), "before the first tick");
    bed.until(30, |s| ready(s) == 2);
    let st = bed.slots.status();
    assert_eq!((st.kinds[0].warm.clone(), st.warm_units), (want(0, 2), 2));
    st.check().unwrap();
    // A degraded host boots no new warm unit, and says why.
    bed.slots.degraded = Some("a residue".into());
    let paused = bed.slots.status().kinds[0].warm.clone().and_then(|w| w.paused_reason);
    assert!(paused.as_deref().is_some_and(|p| p.contains("degraded")), "{paused:?}");
    // A kind with no warm unit in its configuration has no `warm`.
    let cold = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![])],
        vec![KindSpec { name: "fake", slots: 1, capability: confined(), command: sh("exit 0") }],
    );
    let st = cold.slots.status();
    assert_eq!((st.kinds[0].warm.clone(), st.warm_units), (None, 0));
}
