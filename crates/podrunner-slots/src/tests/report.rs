//! T-497 (githome's G-19): the times and the measures of the run report.

use podrunner_schema::{Boundary, EgressHold, ExitReason, Principal, Residual};
use serde_json::json;

use super::*;

fn run(disk_bound: Option<u64>, script: &str) -> podrunner_schema::RunReport {
    ran(disk_bound, script).slots.reports[0].clone()
}

/// The bed after one job of a fake unit that runs `script`.
fn ran(disk_bound: Option<u64>, script: &str) -> Bed {
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
    bed
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
    // The disk check comes each 30 s of the manual clock; the bed ticks
    // each second, and on a slow host the unit can start a tick late.
    assert!(r.run_s.is_some_and(|s| (30..=32).contains(&s)), "the unit ran 30 s until the disk check: {:?}", r.run_s);
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
fn a_job_that_the_job_started_hook_ended_is_a_job_mismatch() {
    // T-552: the hook ends the job and the runner exits 0; the line of the
    // hook in the log of the unit makes the report `job-mismatch`.
    let line = crate::log_tail::HOOK_ENDED;
    let r = run(None, &format!("echo '{line}no-answer' >&2; exit 0"));
    assert_eq!(r.exit_reason, ExitReason::JobMismatch, "{r:?}");
    assert!(r.log_tail.as_deref().is_some_and(|t| t.contains("no-answer")), "{r:?}");
    // With no line, a unit that exits 0 stays a success.
    assert_eq!(run(None, "echo fine; exit 0").exit_reason, ExitReason::Success);
}

#[test]
fn a_runner_that_exits_0_with_no_job_is_no_success() {
    // T-567: `run.sh` of the official runner exits 0 when its listener ends
    // with code 1 (a refused session). The fake payload prints the line of
    // `run-helper.sh` and exits 0, as the runner in a guest did.
    let exit = "echo 'Runner listener exit with terminated error, stop the service, no retry needed.'";
    let bed = ran(None, &format!("{exit}; exit 0"));
    let r = &bed.slots.reports[0];
    assert_eq!(r.exit_reason, ExitReason::UnitFailed, "{r:?}");
    assert_eq!(r.exit_code, Some(0), "{r:?}");
    assert!(r.log_tail.as_deref().is_some_and(|t| t.contains("terminated error")), "{r:?}");
    let why = |l: &crate::audit::Entry| l.detail.as_deref().is_some_and(|d| d.contains("no job"));
    assert!(bed.slots.audit_lines().iter().any(|l| l.audit == crate::audit::Kind::Refuse && why(l)), "no audit line");
    // The runner took a job: its exit 0 stays a success.
    let r = run(None, &format!("echo '2026-11-20 10:00:00Z: Running job: build'; {exit}; exit 0"));
    assert_eq!(r.exit_reason, ExitReason::Success, "{r:?}");
    // No line of the runner (a payload that is not the runner): the exit
    // code tells.
    assert_eq!(run(None, "echo fine; exit 0").exit_reason, ExitReason::Success);
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

#[test]
fn cpu_s_is_the_cpu_time_of_the_payload_within_its_tolerance() {
    // T-520 (githome's G-06): one process burns 2 s of CPU, then the unit
    // ends. The sampler reads it at each tick, and rounds up.
    let started = std::time::Instant::now();
    let mut bed = Bed::new(
        // The fake payload runs no runner: no start event comes, so the
        // deadline of the start must not end it first.
        config("[deadlines]\nstart_s = 100000"),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory),
            command: sh("timeout 2 sh -c 'while :; do :; done'; exit 0"),
        }],
    );
    // Each tick takes 5 ms of real time: the loop needs 2 s.
    bed.until(2000, |s| s.jobs().count() == 1 && Bed::all_final(s));
    let r = bed.slots.reports[0].clone();
    let wall = started.elapsed().as_secs_f64();
    let Some(Residual::Count(cpu)) = r.cpu_s else { panic!("no cpu_s: {r:?}") };
    // The tolerance of the contract: 20 % or 1 s, the larger, of the CPU
    // time of the payload (here at most the 2 s of the loop).
    assert!((1..=3).contains(&cpu), "cpu_s {cpu} for 2 s of CPU in {wall:.1} s");
}
