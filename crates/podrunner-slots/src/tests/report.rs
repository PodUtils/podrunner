//! T-497 (githome's G-19): the times and the measures of the run report.

use podrunner_schema::{Boundary, EgressHold, ExitReason, Principal, Residual};
use serde_json::json;

use super::*;

fn run(disk_bound: Option<u64>, script: &str) -> podrunner_schema::RunReport {
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory),
            command: sh(script),
        }],
    );
    bed.slots.kinds[0].cfg.disk_max_bytes = disk_bound;
    bed.until(120, |s| s.jobs().count() == 1 && Bed::all_final(s));
    bed.slots.reports[0].clone()
}

#[test]
fn a_report_has_its_times_its_image_and_its_measures_or_a_reason() {
    // A unit over a small disk bound: a disk check reads its peak, and the
    // unit ends by `limit`.
    let r = run(Some(1000), "head -c 5000 /dev/zero > some.bin; exec sleep 300");
    for (name, v) in [("prepare_s", r.prepare_s), ("run_s", r.run_s), ("destroy_s", r.destroy_s)] {
        assert!(v.is_some(), "{name}: {r:?}");
    }
    // The fake unit runs no job: no first step, so no start_s, with a reason.
    assert_eq!(r.start_s, None);
    assert_eq!(r.unknown_reasons.get("start_s").map(String::as_str), Some("the runner reached no first step"));
    assert_eq!(r.run_s, Some(30), "the unit ran 30 s of the manual clock until the disk check");
    assert!(r.image_digest.as_deref().is_some_and(|d| d.starts_with("sha256:")), "{r:?}");
    assert!(matches!(r.disk_peak_bytes, Some(Residual::Count(n)) if n >= 5000), "{:?}", r.disk_peak_bytes);
    // No egress module on this host: unknown, with the reason.
    assert!(matches!(r.net_rx_bytes, Some(Residual::Unknown(_))));
    assert_eq!(r.unknown_reasons.get("net_rx_bytes").map(String::as_str), Some("the host has no egress module"));
    // No disk bound: no disk check, so unknown with its reason.
    let r = run(None, "exit 0");
    assert!(matches!(r.disk_peak_bytes, Some(Residual::Unknown(_))));
    assert!(r.unknown_reasons.contains_key("disk_peak_bytes"));
}

#[test]
fn a_unit_that_fails_before_a_step_gives_a_scrubbed_log_tail() {
    // The payload prints its credential file and fails: the canary of the
    // credential is not in the tail, and the rest of the log is.
    let r = run(None, "echo before; cat .runner-root/.credentials; echo; echo after; exit 3");
    let tail = r.log_tail.as_deref().expect("a log tail");
    assert!(tail.contains("before") && tail.contains("after") && tail.contains("clientId"), "{tail}");
    assert!(!tail.contains("fake-client"), "the canary of the credential is in the tail: {tail}");
    // A unit that ends well gives none.
    assert_eq!(run(None, "echo fine; exit 0").log_tail, None);
}

#[test]
fn a_job_that_failed_after_an_oom_kill_ends_with_limit() {
    // T-515: the fake unit stands for a kind that says that the OOM killer
    // ended a process of the unit; the job failed after it.
    let r = run(None, &format!("echo 1 > {}; exit 3", podrunner_fake::OOM_KILLS));
    assert_eq!(r.exit_reason, ExitReason::Limit, "{r:?}");
    // No OOM kill: the failure stays a failure of the unit.
    let r = run(None, &format!("echo 0 > {}; exit 3", podrunner_fake::OOM_KILLS));
    assert_ne!(r.exit_reason, ExitReason::Limit, "{r:?}");
}
