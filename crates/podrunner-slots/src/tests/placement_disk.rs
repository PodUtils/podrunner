//! T-571: a kind with `root_quota` sizes the root disk of a unit by the
//! lesser of the `disk` limit of the offer and `disk_max_bytes`. Placement
//! refuses an offer whose bound is below the unpacked size of its payload,
//! with both sizes, and places one at the size.

use podrunner_schema::{Boundary, EgressHold, JobOffer, Principal, RefusalCode, Tier};
use serde_json::json;

use super::*;
use crate::placement::{Kind, place};

const SIZE: u64 = 30_000_000_000;

fn job(disk: Option<u64>) -> JobOffer {
    let with = disk.map_or_else(|| json!({}), |d| json!({ "limits": { "disk": d } }));
    let line: Value = serde_json::from_str(&offer(0, "j-1", with)).unwrap();
    serde_json::from_value(line["offer"].clone()).unwrap()
}

fn kind(root_disk: Option<u64>) -> Kind {
    let c = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    Kind { capability: c, free: 1, running: 0, repos: vec![], root_disk }
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
