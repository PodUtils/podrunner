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
    // The fake unit runs no job: no first step, so no start_s, and the
    // reason says why (T-595, `pod_job::report::TIMES`).
    assert_eq!(r.start_s, None);
    assert!(r.unknown_reasons.get("start_s").is_some_and(|w| w.contains("first step")), "{r:?}");
    assert_eq!(r.claim_to_first_step_s, None);
    assert!(r.unknown_reasons.contains_key("claim_to_first_step_s"), "{r:?}");
    // The disk check comes each 30 s of the manual clock; the bed ticks
    // each second, and on a slow host the unit can start a tick late.
    assert!(r.run_s.is_some_and(|s| (30..=32).contains(&s)), "the unit ran 30 s until the disk check: {:?}", r.run_s);
    assert!(r.image_digest.as_deref().is_some_and(|d| d.starts_with("sha256:")), "{r:?}");
    assert!(matches!(r.disk_peak_bytes, Some(Residual::Count(n)) if n >= 5000), "{:?}", r.disk_peak_bytes);
    // No egress module on this host: unknown, with the reason.
    assert!(matches!(r.net_rx_bytes, Some(Residual::Unknown(_))));
    assert_eq!(r.unknown_reasons.get("net_rx_bytes").map(String::as_str), Some("the host has no egress module"));
    each_measure_or_its_reason(&r);
    // No disk bound: no disk check, so unknown with its reason.
    let r = run(None, "exit 0");
    assert!(matches!(r.disk_peak_bytes, Some(Residual::Unknown(_))));
    assert!(r.unknown_reasons.contains_key("disk_peak_bytes"));
    // No shared cache yet (T-480): unknown with its reason.
    assert!(matches!(r.cache_read_bytes, Some(Residual::Unknown(_))), "{r:?}");
    assert!(r.unknown_reasons.get("cache_written_bytes").is_some_and(|w| w.contains("T-480")), "{r:?}");
    each_measure_or_its_reason(&r);
}

/// Each measured field of the report of a unit that ran is a count, or
/// `unknown` with its reason; the report passes the rules of `pod-job`.
fn each_measure_or_its_reason(r: &podrunner_schema::RunReport) {
    let fields = [
        ("cpu_s", r.cpu_s),
        ("peak_rss_mib", r.peak_rss_mib),
        ("disk_peak_bytes", r.disk_peak_bytes),
        ("net_rx_bytes", r.net_rx_bytes),
        ("net_tx_bytes", r.net_tx_bytes),
        ("cache_read_bytes", r.cache_read_bytes),
        ("cache_written_bytes", r.cache_written_bytes),
    ];
    for (name, v) in fields {
        match v {
            Some(Residual::Count(_)) => {}
            Some(Residual::Unknown(_)) => assert!(r.unknown_reasons.contains_key(name), "{name} has no reason: {r:?}"),
            None => panic!("{name} is absent: {r:?}"),
        }
    }
    // Each absent time has its reason; a present time has none (T-595).
    let times = [
        ("claim_to_first_step_s", r.claim_to_first_step_s),
        ("prepare_s", r.prepare_s),
        ("start_s", r.start_s),
        ("run_s", r.run_s),
        ("destroy_s", r.destroy_s),
    ];
    assert_eq!(times.map(|(name, _)| name), podrunner_schema::report::TIMES);
    for (name, v) in times {
        assert_eq!(v.is_none(), r.unknown_reasons.contains_key(name), "{name}: {r:?}");
    }
    r.check().unwrap();
}

#[test]
fn a_unit_that_fails_before_a_step_gives_a_scrubbed_log_tail() {
    // The payload prints its credential file and fails: the canary of the
    // credential is not in the tail, and the rest of the log is.
    let r = run(None, "echo before; cat .runner-root/.credentials; echo; echo after; exit 3");
    let tail = r.log_tail.as_deref().expect("a log tail");
    assert!(tail.contains("before") && tail.contains("after") && tail.contains("clientId"), "{tail}");
    assert!(!tail.contains("fake-client"), "the canary of the credential is in the tail: {tail}");
    each_measure_or_its_reason(&r);
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
    // The runner took a job and it succeeded: its exit 0 stays a success.
    let ok = "echo '2026-11-20 10:00:09Z: Job build completed with result: Succeeded'";
    let r = run(None, &format!("echo '2026-11-20 10:00:00Z: Running job: build'; {ok}; {exit}; exit 0"));
    assert_eq!(r.exit_reason, ExitReason::Success, "{r:?}");
    // No line of the runner (a payload that is not the runner): the exit
    // code tells.
    assert_eq!(run(None, "echo fine; exit 0").exit_reason, ExitReason::Success);
}

#[test]
fn a_runner_that_exits_0_after_a_failed_job_is_no_success() {
    // T-568: `run.sh` exits 0 whatever the result of the job. The fake
    // payload prints the lines of `JobDispatcher.cs` and exits 0.
    let job = |result: &str| run(None, &lines_of(result));
    let r = job("Failed");
    assert_eq!(r.exit_reason, ExitReason::Failure, "{r:?}");
    assert_eq!(r.exit_code, Some(0), "{r:?}");
    // No first step: the unit failed before it, so it has its log tail
    // (T-497).
    assert!(r.log_tail.as_deref().is_some_and(|t| t.contains("result: Failed")), "{r:?}");
    let r = job("Canceled");
    assert_eq!(r.exit_reason, ExitReason::Cancelled, "{r:?}");
    assert_eq!(r.log_tail, None, "a cancel is no failure: {r:?}");
    let r = job("Succeeded");
    assert_eq!(r.exit_reason, ExitReason::Success, "{r:?}");
    assert_eq!(r.log_tail, None, "{r:?}");
    // A step that prints the words is not the line of the runner.
    let words = "echo 'completed with result: Failed'";
    let r = run(None, &format!("{words}; {}", lines_of("Succeeded")));
    assert_eq!(r.exit_reason, ExitReason::Success, "{r:?}");
}

#[test]
fn a_forged_running_line_does_not_hide_a_failed_job_and_the_change_has_an_audit_line() {
    // T-568: a step writes a later `Running job:` line of another name to
    // the console of the listener; the real `Failed` line still tells.
    let lines = "echo '2026-11-20 10:00:00Z: Running job: build'; \
                 echo '2026-11-20 10:00:03Z: Running job: x'; \
                 echo '2026-11-20 10:00:09Z: Job build completed with result: Failed'; \
                 echo '2026-11-20 10:00:10Z: Job build completed with result: Succeeded'; exit 0";
    let bed = ran(None, lines);
    let r = &bed.slots.reports[0];
    assert_eq!(r.exit_reason, ExitReason::Failure, "{r:?}");
    let why = |l: &crate::audit::Entry| l.detail.as_deref().is_some_and(|d| d.ends_with("result: Failed"));
    let lines = bed.slots.audit_lines();
    assert!(lines.iter().any(|l| l.audit == crate::audit::Kind::JobCompleted && why(l)), "{lines:?}");
    // A success changes no reason: no such audit line.
    let bed = ran(None, &lines_of("Succeeded"));
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success);
    assert!(!bed.slots.audit_lines().iter().any(|l| l.audit == crate::audit::Kind::JobCompleted));
}

#[test]
fn a_runner_that_exits_0_after_an_abandoned_job_is_no_success() {
    // T-569: the listener prints `Abandoned` when the renewal of the lock
    // of the job ends before the job, and prints no completed line when
    // the first renewal fails. Both exit 0.
    let completed = |l: &crate::audit::Entry| l.audit == crate::audit::Kind::JobCompleted;
    let bed = ran(None, &lines_of("Abandoned"));
    let r = &bed.slots.reports[0];
    assert_eq!(r.exit_reason, ExitReason::Failure, "{r:?}");
    let why = |l: &crate::audit::Entry| l.detail.as_deref().is_some_and(|d| d.ends_with("result: Abandoned"));
    assert!(bed.slots.audit_lines().iter().any(|l| completed(l) && why(l)), "no audit line");
    let bed = ran(None, "echo '2026-11-20 10:00:00Z: Running job: build'; exit 0");
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Failure);
    let why = |l: &crate::audit::Entry| l.detail.as_deref().is_some_and(|d| d.contains("no completed line"));
    assert!(bed.slots.audit_lines().iter().any(|l| completed(l) && why(l)), "no audit line");
    // A result that the forge counts as no failure stays a success, and
    // it does not hide a failure.
    assert_eq!(run(None, &lines_of("SucceededWithIssues")).exit_reason, ExitReason::Success);
    let forged = "echo '2026-11-20 10:00:10Z: Job build completed with result: SucceededWithIssues'";
    assert_eq!(run(None, &format!("{}; {forged}", lines_of("Failed"))).exit_reason, ExitReason::Failure);
}

#[test]
fn a_part_line_of_a_process_of_the_job_does_not_hide_the_failed_line() {
    // T-569: a process of the job writes to the console of the listener
    // with no new line, so the real `Failed` line ends that line; then it
    // forges a `Succeeded` line.
    let lines = "echo '2026-11-20 10:00:00Z: Running job: build'; printf x; \
                 echo '2026-11-20 10:00:09Z: Job build completed with result: Failed'; \
                 echo '2026-11-20 10:00:10Z: Job build completed with result: Succeeded'; exit 0";
    assert_eq!(run(None, lines).exit_reason, ExitReason::Failure);
}

#[test]
fn a_log_over_the_bound_tells_a_failure_before_the_cut_and_no_more() {
    // T-570: slots reads at most the bound of the log. A process of the
    // job writes more than the bound after the real `Failed` line, then a
    // forged `Succeeded` line.
    crate::report::LOG_READ_BOUND.set(1 << 20);
    let run_line = "echo '2026-11-20 10:00:00Z: Running job: build'";
    let failed = "echo '2026-11-20 10:00:09Z: Job build completed with result: Failed'";
    let ok = "echo '2026-11-20 10:00:10Z: Job build completed with result: Succeeded'";
    let more = "yes 'a line of the job' | head -c 3000000; echo";
    let r = run(None, &format!("{run_line}; {failed}; {more}; {ok}; exit 0"));
    assert_eq!(r.exit_reason, ExitReason::Failure, "{r:?}");
    // The real line after the bound is not read: the exit code tells, and
    // an audit line names the cut. With no completed line in the part
    // that was read, a cut log is no failure.
    let cut = |l: &crate::audit::Entry| l.detail.as_deref().is_some_and(|d| d.contains("bound of 1048576 bytes"));
    for tail in [failed, "true"] {
        let bed = ran(None, &format!("{run_line}; {more}; {tail}; exit 0"));
        assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success, "{tail}");
        assert!(bed.slots.audit_lines().iter().any(cut), "no audit line of the cut: {tail}");
    }
}

#[test]
fn a_log_that_cannot_be_read_keeps_the_exit_code_and_says_why() {
    // T-570: the read of the log fails (the incus tier of a podvm before
    // `0.2.0-beta.38` refuses a log over 64 MiB; here the fake unit has no log file). The report
    // keeps the exit code, and an audit line names the failed read.
    let bed = ran(None, "echo '2026-11-20 10:00:00Z: Running job: build'; rm -f .unit.log; exit 0");
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success);
    let said = |l: &crate::audit::Entry| {
        l.detail.as_deref().is_some_and(|d| d.starts_with("the log of the unit tells no result of the job: "))
    };
    assert!(bed.slots.audit_lines().iter().any(said), "{:?}", bed.slots.audit_lines());
}

fn lines_of(result: &str) -> String {
    format!(
        "echo '2026-11-20 10:00:00Z: Running job: build'; \
         echo '2026-11-20 10:00:09Z: Job build completed with result: {result}'; exit 0"
    )
}

#[test]
fn the_listener_of_a_unit_copies_no_worker_output_to_the_log() {
    // T-568: the start environment sets the variable to "false", so the
    // log that `job_result` reads holds no trace line of the worker.
    let r = run(None, r#"test "$ACTIONS_RUNNER_PRINT_LOG_TO_STDOUT" = false || exit 3"#);
    assert_eq!(r.exit_reason, ExitReason::Success, "{r:?}");
}

#[test]
fn a_job_that_failed_after_an_oom_kill_ends_with_limit() {
    // T-515: the fake unit stands for a kind that says that the OOM killer
    // ended a process of the unit; the job failed after it.
    let r = run(None, &format!("echo 1 > {}; exit 3", podrunner_fake::OOM_KILLS));
    assert_eq!(r.exit_reason, ExitReason::Limit, "{r:?}");
    each_measure_or_its_reason(&r);
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
