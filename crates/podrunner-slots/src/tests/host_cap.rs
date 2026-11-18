//! T-505: `max_units` caps the units of the host across its kinds; each
//! kind can still take each unit of the cap.

use podrunner_schema::{Boundary, EgressHold, Principal};
use serde_json::json;

use super::*;

fn kinds() -> Vec<KindSpec> {
    let cap = || capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    vec![
        KindSpec { name: "fake", slots: 2, capability: cap(), command: sh("exec sleep 300") },
        KindSpec { name: "fake", slots: 2, capability: cap(), command: sh("exec sleep 300") },
    ]
}

fn running(s: &Slots) -> Vec<usize> {
    s.jobs()
        .filter(|j| j.unit.is_some() && !j.state.is_final())
        .filter_map(|j| j.placement.as_ref().map(|p| p.kind))
        .collect()
}

#[test]
fn the_cap_of_the_host_holds_across_its_kinds() {
    let offers: Vec<String> = (1..=4).map(|n| offer(0, &format!("j-{n}"), json!({}))).collect();
    let mut bed = Bed::new(config("max_units = 2"), vec![("a", "trusted", None, offers)], kinds());
    bed.until(30, |s| running(s).len() == 2);
    for _ in 0..5 {
        bed.tick();
        assert_eq!(running(&bed.slots).len(), 2, "never more than the cap: {:?}", bed.states());
    }
    // The first kind took both units of the cap: a kind is bounded by the
    // cap, not by a share of it.
    assert_eq!(running(&bed.slots), [0, 0]);
    let st = bed.slots.status();
    assert_eq!((st.max_units, st.units), (Some(2), 2));
    // With no cap, the four jobs run on the two kinds.
    let offers: Vec<String> = (1..=4).map(|n| offer(0, &format!("j-{n}"), json!({}))).collect();
    let mut bed = Bed::new(config(""), vec![("a", "trusted", None, offers)], kinds());
    bed.until(30, |s| running(s).len() == 4);
    assert!(Config::parse("host = \"h\"\nmax_units = 0\n[pin]\n").unwrap_err().contains("max_units = 0"));
}
