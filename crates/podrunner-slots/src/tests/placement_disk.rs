//! T-571: a kind with `root_quota` sizes the root disk of a unit by the
//! lesser of the `disk` limit of the offer and `disk_max_bytes`. Placement
//! refuses an offer whose bound is below the unpacked size of its payload,
//! with both sizes, and places one at the size.

use podrunner_schema::{Boundary, EgressHold, ExitReason, JobOffer, Principal, RefusalCode, Residual, Tier, UnitId};
use serde_json::json;

use super::*;
use crate::audit;
use crate::disk::CHECK_MS;
use crate::placement::{Kind, place};

const SIZE: u64 = 30_000_000_000;

fn job(disk: Option<u64>) -> JobOffer {
    let with = disk.map_or_else(|| json!({}), |d| json!({ "limits": { "disk": d } }));
    let line: Value = serde_json::from_str(&offer(0, "j-1", with)).unwrap();
    serde_json::from_value(line["offer"].clone()).unwrap()
}

fn kind(root_disk: Option<u64>) -> Kind {
    let c = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    Kind {
        engine_needs_payload: false,
        capability: c,
        free: 1,
        running: 0,
        repos: vec![],
        root_disk,
        init: None,
        warm: vec![],
    }
}

/// The configuration of the tests, with a pinned payload of `SIZE` bytes.
fn sized() -> Config {
    let mut cfg = config("");
    cfg.pin.payload[0].unpacked_bytes = Some(SIZE);
    cfg
}

#[test]
fn placement_refuses_a_root_disk_below_the_payload() {
    let cfg = sized();
    let quota = [kind(Some(40_000_000_000))];
    let at = |disk, kinds: &[Kind]| place(&job(disk), Tier::Trusted, kinds, &cfg, NOW_UNIX).map(|p| p.kind);
    assert_eq!(at(Some(SIZE - 1), &quota), Err(RefusalCode::LimitNotEnforceable));
    assert_eq!(at(Some(SIZE), &quota), Ok(0), "at the size");
    assert_eq!(at(None, &quota), Ok(0), "the bound of the kind holds it");
    assert_eq!(at(None, &[kind(Some(SIZE - 1))]), Err(RefusalCode::LimitNotEnforceable));
    // A kind that does not size the root takes the offer.
    assert_eq!(at(Some(SIZE - 1), &[kind(Some(40_000_000_000)), kind(None)]), Ok(1));
    // A pin with no size is not checked.
    assert_eq!(place(&job(Some(1)), Tier::Trusted, &quota, &config(""), NOW_UNIX).map(|p| p.kind), Ok(0));
}

/// Through the slots: the audit line of the refusal names both sizes, and
/// an offer at the size starts its unit.
#[test]
fn placement_names_both_sizes_in_the_audit_line() {
    let run = |disk: u64| {
        let lines = vec![offer(0, "j-1", json!({ "limits": { "disk": disk } }))];
        let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
        let mut bed = Bed::new(
            sized(),
            vec![("a", "trusted", None, lines)],
            vec![KindSpec { name: "fake", slots: 1, capability: cap, command: sh("exit 0") }],
        );
        bed.slots.kinds[0].cfg.root_quota = true;
        bed.until(20, |s| s.jobs().count() == 1 && Bed::all_final(s));
        bed
    };
    let bed = run(SIZE - 1);
    assert_eq!(bed.slots.reports[0].refusal_code, Some(RefusalCode::LimitNotEnforceable));
    let refuse = bed.slots.audit_lines().iter().find(|a| a.audit == crate::audit::Kind::Refuse).unwrap();
    let detail = refuse.detail.as_deref().unwrap_or_default();
    assert!(detail.contains(&format!("the root disk of a unit is {} bytes", SIZE - 1)), "{detail}");
    assert!(detail.contains(&format!("less than the {SIZE} bytes of the payload {PIN} unpacked")), "{detail}");
    let bed = run(SIZE);
    assert_eq!(bed.slots.reports[0].refusal_code, None, "{:?}", bed.slots.reports[0]);
    assert!(bed.slots.audit_lines().iter().all(|a| a.audit != crate::audit::Kind::Refuse));
}

fn sample_bed() -> Bed {
    let lines = vec![
        offer(0, "j-1", json!({})),
        format!(r#"{{"at_s":{},"started":{{"job_id":"j-1","runner_name":"podrunner-host-a-u-0"}}}}"#, CHECK_MS / 1000),
    ];
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, lines)],
        vec![KindSpec { name: "fake", slots: 1, capability: cap, command: sh("exec sleep 300") }],
    );
    let kind = &mut bed.slots.kinds[0];
    kind.cfg.disk_max_bytes = Some(8192);
    bed.until(10, |s| s.jobs().any(|j| j.state == State::Started));
    let unit = bed.slots.jobs().next().unwrap().unit.as_ref().unwrap();
    let root = bed.slots.kinds[0].driver.observe(unit).unwrap().unit_root.unwrap();
    let tree = root.join("tree");
    std::fs::create_dir(&tree).unwrap();
    for i in 0..256 {
        std::fs::File::create(tree.join(i.to_string())).unwrap();
    }
    std::fs::write(tree.join("large"), [0; 16_384]).unwrap();
    bed.clock.advance(Duration::from_millis(CHECK_MS - bed.slots.now()));
    bed
}

#[test]
fn a_tick_takes_the_started_event_before_the_disk_result() {
    let mut bed = sample_bed();
    bed.slots.tick();
    let job = bed.slots.jobs().next().unwrap();
    assert_eq!(job.state, State::Running);
    assert_eq!(job.term_ms, None, "the tick that sends the walk cannot have its result");
    bed.until(0, Bed::all_final);
    let r = &bed.slots.reports[0];
    assert_eq!(r.exit_reason, ExitReason::Limit, "{r:?}");
    assert!(matches!(r.disk_peak_bytes, Some(Residual::Count(n)) if n >= 16_384), "{r:?}");
    let limits: Vec<_> = bed.slots.audit_lines().iter().filter(|a| a.audit == audit::Kind::Limit).collect();
    assert_eq!(limits.len(), 1);
    assert_eq!(limits[0].detail.as_deref(), Some("disk"));
}

#[test]
fn a_pending_disk_walk_does_not_hold_ticks_or_queue_more_walks() {
    let mut bed = sample_bed();
    let gate = bed.slots.disk_worker.gate.clone();
    let held = gate.lock().unwrap();
    for _ in 0..3 {
        bed.slots.tick();
        assert_eq!(bed.slots.disk_worker.pending, 1);
        let job = bed.slots.jobs().next().unwrap();
        assert_eq!(job.state, State::Running);
        assert_eq!(job.term_ms, None);
        bed.clock.advance(Duration::from_millis(CHECK_MS));
    }
    drop(held);
    bed.until(0, Bed::all_final);
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Limit);
}

#[test]
fn a_disk_result_of_an_old_unit_does_not_end_its_replacement() {
    let mut bed = sample_bed();
    let gate = bed.slots.disk_worker.gate.clone();
    let held = gate.lock().unwrap();
    bed.slots.tick();
    assert_eq!(bed.slots.disk_worker.pending, 1);
    let job = bed.slots.jobs.values_mut().next().unwrap();
    // Model a new unit of the same job while the old sample waits.
    job.unit = Some(UnitId::parse("u-42").unwrap());
    drop(held);
    bed.until(0, |s| s.disk_worker.pending == 0);
    let job = bed.slots.jobs().next().unwrap();
    assert_eq!(job.state, State::Running);
    assert_eq!(job.term_ms, None);
    assert_eq!(job.times.disk_peak, None);
    assert!(bed.slots.audit_lines().iter().all(|a| a.audit != audit::Kind::Limit));
}
