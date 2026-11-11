//! The level 1 asks of githome: G-01, G-02, G-05, G-08 and G-12.

use podrunner_schema::{Boundary, EgressHold, ExitReason, Principal, RefusalCode};
use serde_json::json;

use super::*;

fn redirect() -> podrunner_schema::Capability {
    // `trusted` needs `confined` (DR-4 is open): the fake declares it.
    capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory)
}

/// G-01: two sources with 2 slots each, 10 jobs each; none holds more than
/// its slots at any time, and each job ends.
#[test]
fn g01_no_source_holds_more_than_its_slots() {
    let jobs = |p: &str| (0..10).map(|i| offer(0, &format!("{p}-{i}"), json!({}))).collect::<Vec<_>>();
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", Some(2), jobs("a")), ("b", "trusted", Some(2), jobs("b"))],
        vec![KindSpec { name: "fake", slots: 8, capability: redirect(), command: sh("exit 0") }],
    );
    let mut peak = [0usize; 2];
    for _ in 0..200 {
        bed.tick();
        for (s, p) in peak.iter_mut().enumerate() {
            let held = bed.slots.jobs().filter(|j| j.source == s && j.unit.is_some() && !j.state.is_final()).count();
            assert!(held <= 2, "source {s} holds {held} units");
            *p = (*p).max(held);
        }
        if bed.slots.jobs().count() == 20 && Bed::all_final(&bed.slots) {
            break;
        }
    }
    assert_eq!(peak, [2, 2]);
    assert_eq!(bed.slots.reports.iter().filter(|r| r.exit_reason == ExitReason::Success).count(), 20);
}

/// G-02: a job that needs `guest` waits on a host whose guest slot is
/// busy, while other slots are free, and runs when the guest slot frees.
#[test]
fn g02_a_job_waits_for_its_kind() {
    let guest = capability(Boundary::Guest, Principal::Distinct, EgressHold::Enforced);
    let need = json!({ "unit_need": { "min_boundary": "guest" } });
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "first", need.clone()), offer(2, "second", need)])],
        vec![
            KindSpec { name: "fake", slots: 4, capability: redirect(), command: sh("exit 0") },
            KindSpec { name: "fake-guest", slots: 1, capability: guest, command: sh("sleep 2") },
        ],
    );
    bed.until(20, |s| s.jobs().any(|j| j.id.as_str() == "second"));
    assert_eq!(bed.state("second"), Some(State::Offered), "it waits while the guest slot is busy");
    bed.until(400, |s| s.jobs().all(|j| j.state == State::Reported));
    let kinds: Vec<_> = bed.slots.reports.iter().map(|r| r.unit.as_ref().unwrap().unit_kind.clone()).collect();
    assert_eq!(kinds, ["fake-guest", "fake-guest"]);
}

/// G-05: an arm64 job is not claimed on a host with x86_64 kinds only.
#[test]
fn g05_an_arm64_job_is_not_claimed() {
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "arm", json!({ "arch": "aarch64" }))])],
        vec![KindSpec { name: "fake", slots: 1, capability: redirect(), command: sh("exit 0") }],
    );
    bed.until(10, |s| s.jobs().count() == 1 && Bed::all_final(s));
    assert_eq!(bed.slots.reports[0].refusal_code, Some(RefusalCode::ArchMismatch));
    assert!(bed.slots.audit_lines().iter().all(|a| a.audit != crate::audit::Kind::Claim));
}

/// G-08: one withdraw for each job past `wait_deadline_s`; a withdrawn job
/// never starts; after `drain`, no job is claimed.
#[test]
fn g08_withdraw_at_the_deadline_and_drain() {
    // Each job needs the one guest slot; the free slots of another kind
    // give the capacity that brings the offers in.
    let wait = json!({ "wait_deadline_s": 10, "unit_need": { "min_boundary": "guest" } });
    let lines = vec![
        offer(0, "long", json!({ "unit_need": { "min_boundary": "guest" } })),
        offer(0, "w1", wait.clone()),
        offer(0, "w2", wait),
        offer(30, "late", json!({})),
    ];
    let guest = capability(Boundary::Guest, Principal::Distinct, EgressHold::Enforced);
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, lines)],
        vec![
            KindSpec { name: "fake-guest", slots: 1, capability: guest, command: sh("sleep 300") },
            KindSpec { name: "fake", slots: 4, capability: redirect(), command: sh("exit 0") },
        ],
    );
    bed.until(20, |s| s.jobs().filter(|j| j.state == State::Released).count() == 2);
    for w in ["w1", "w2"] {
        let r = bed.slots.reports.iter().find(|r| r.job_id.as_str() == w).unwrap();
        assert_eq!(r.exit_reason, ExitReason::Withdrawn);
        assert!(r.unit.is_none());
    }
    bed.slots.drain();
    for _ in 0..20 {
        bed.tick();
    }
    assert_eq!(bed.state("late"), None, "a draining host takes no offer");
    let claims = bed.slots.audit_lines().iter().filter(|a| a.audit == crate::audit::Kind::Claim).count();
    assert_eq!(claims, 1);
}

/// G-12: a job past its wall time ends `timeout`, and its unit is gone
/// within 60 s.
#[test]
fn g12_a_job_past_its_wall_time_ends() {
    let wall = json!({ "limits": { "wall": 5 }, "limits_required": ["wall"] });
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "slow", wall)])],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: redirect(),
            command: sh("trap '' TERM; setsid sleep 300 & sleep 300"),
        }],
    );
    bed.until(10, |s| s.jobs().any(|j| j.state == State::Started));
    let ticks = bed.until(60, Bed::all_final);
    assert!(ticks <= 60);
    let r = &bed.slots.reports[0];
    assert_eq!(r.exit_reason, ExitReason::Timeout);
    let u = r.unit.as_ref().unwrap();
    assert_eq!(u.residual_processes, podrunner_schema::Residual::Count(0));
    assert!(bed.slots.degraded().is_none());
}
