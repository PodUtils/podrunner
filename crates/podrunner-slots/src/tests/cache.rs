//! T-480 (githome's G-07 and G-20, cfb's P-3): the cache of `serve`, kept
//! apart by trust. A push to the default branch writes it; a pull request
//! of the same repository reads it and does not write it; an untrusted job
//! (a fork) gets none, also when its offer asks for `rw`.

use podrunner_schema::{Boundary, EgressHold, Principal, Residual};
use serde_json::json;

use super::*;

/// The script of each job: the run id in the job file tells which.
fn script(out: &std::path::Path) -> String {
    let out = out.display();
    format!(
        r#"run=$(cut -d' ' -f2 .runner-root/.podrunner-job)
case $run in
1) echo push > "$PODRUNNER_CACHE/from-push"; echo "$PODRUNNER_CACHE_MODE" > {out}/push-mode
   test -d "$npm_config_cache" && echo ok > {out}/push-env ;;
2) echo "${{PODRUNNER_CACHE:-none}} ${{npm_config_cache:-none}}" > {out}/fork-env
   echo evil > "${{PODRUNNER_CACHE:-{out}}}/evil" ;;
3) cat "$PODRUNNER_CACHE/from-push" > {out}/pr-saw; echo "$PODRUNNER_CACHE_MODE" > {out}/pr-mode
   echo "${{npm_config_cache:-none}}" > {out}/pr-env
   echo pr > "$PODRUNNER_CACHE/from-pr" ;;
4) ls -A "$PODRUNNER_CACHE" > {out}/last ;;
esac
exit 0"#
    )
}

/// A capability of the fake unit that holds `:ro` (a copy of the cache).
fn ro_capability(boundary: Boundary) -> podrunner_schema::Capability {
    let mut cap = capability(boundary, Principal::Distinct, EgressHold::Enforced);
    cap.mount_modes = vec!["ro".into(), "rw".into()];
    cap
}

fn files_under(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        out.push(p.file_name().unwrap().to_string_lossy().into_owned());
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            out.extend(files_under(&p));
        }
    }
    out
}

#[test]
fn an_untrusted_write_is_not_read_by_a_trusted_job_and_a_pull_request_does_not_write() {
    let dir = TempDir::new();
    let (cache, out) = (dir.0.join("cache"), dir.0.join("out"));
    std::fs::create_dir_all(&out).unwrap();
    let cfg = config(&format!("[cache]\ndir = \"{}\"\nenv = {{ npm_config_cache = \"npm\" }}", cache.display()));
    let push = |at, run| offer(at, &format!("j-{run}"), json!({ "trust_inputs": { "run_id": run } }));
    let fork = offer(
        30,
        "j-2",
        json!({
            "trust_inputs": { "run_id": 2, "event": "pull_request", "head_is_fork": "yes", "actor_class": "outside",
                              "ref": "refs/pull/9/merge" },
            "cache": { "key": "default", "mode": "rw" }
        }),
    );
    let pr = offer(
        60,
        "j-3",
        json!({
            "trust_inputs": { "run_id": 3, "event": "pull_request", "ref": "refs/pull/7/merge" },
            "cache": { "key": "default", "mode": "rw" }
        }),
    );
    let mut bed = Bed::in_dir(
        cfg,
        vec![("t", "trusted", None, vec![push(0, 1), pr, push(90, 4)]), ("u", "untrusted", None, vec![fork])],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: ro_capability(Boundary::Guest),
            command: sh(&script(&out)),
        }],
        dir,
        Bed::clock(),
    );
    bed.until(240, |s| s.jobs().count() == 4 && Bed::all_final(s));
    let left = files_under(&cache);
    assert!(!left.iter().any(|f| f == "evil" || f == "from-pr"), "{left:?}");
    let read = |name: &str| std::fs::read_to_string(out.join(name)).unwrap_or_default();
    assert_eq!((read("push-mode"), read("push-env")), ("rw\n".into(), "ok\n".into()), "the push writes");
    assert_eq!(read("fork-env"), "none none\n", "the fork gets no cache");
    assert_eq!((read("pr-saw"), read("pr-mode")), ("push\n".into(), "ro\n".into()), "the pull request reads");
    // A tool writes in the directory of its variable: a `ro` job gets none.
    assert_eq!(read("pr-env"), "none\n", "no variable of [cache] env for ro");
    assert_eq!(read("last").lines().collect::<Vec<_>>(), ["from-push", "npm"], "the next push");
    let report = |job: &str| bed.slots.reports.iter().find(|r| r.job_id.as_str() == job).cloned().unwrap();
    let written = |job: &str| report(job).cache_written_bytes;
    assert_eq!(written("j-1"), Some(Residual::Count(5)), "{:?}", report("j-1"));
    assert_eq!((written("j-2"), report("j-2").cache_read_bytes), (Some(Residual::Count(0)), Some(Residual::Count(0))));
    assert_eq!(written("j-3"), Some(Residual::Count(0)));
    let r = report("j-3");
    assert!(matches!(r.cache_read_bytes, Some(Residual::Unknown(_))), "{r:?}");
    assert!(r.unknown_reasons.get("cache_read_bytes").is_some_and(|w| w.contains("T-649")), "{r:?}");
}

/// With no `[cache]`, a job gets no cache, and its report says 0 bytes.
#[test]
fn a_job_with_no_cache_reads_and_writes_0_bytes() {
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({ "cache": { "key": "k", "mode": "rw" } }))])],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory),
            command: sh("test -z \"$PODRUNNER_CACHE\""),
        }],
    );
    bed.until(60, |s| s.jobs().count() == 1 && Bed::all_final(s));
    let r = &bed.slots.reports[0];
    assert_eq!(r.exit_reason, podrunner_schema::ExitReason::Success, "{r:?}");
    assert_eq!((r.cache_read_bytes, r.cache_written_bytes), (Some(Residual::Count(0)), Some(Residual::Count(0))));
}

fn one_job(cfg: Config, script: &str) -> Bed {
    Bed::new(
        cfg,
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory),
            command: sh(script),
        }],
    )
}

/// A `rw` job that fills its cache above `max_bytes` while it runs ends
/// with `limit`: the cache is on the file system of the state.
#[test]
fn a_rw_job_above_max_bytes_ends_while_it_runs() {
    let cfg = config("[cache]\nmax_bytes = 1000\ntotal_bytes = 1000");
    let mut bed = one_job(cfg, "head -c 4096 /dev/zero > \"$PODRUNNER_CACHE/big\"; exec sleep 300");
    bed.until(90, |s| s.jobs().count() == 1 && Bed::all_final(s));
    let r = &bed.slots.reports[0];
    assert_eq!(r.exit_reason, podrunner_schema::ExitReason::Limit, "{r:?}");
    let line = bed.slots.audit_lines().iter().find(|a| a.audit == crate::audit::Kind::Limit).expect("a limit line");
    assert_eq!(line.detail.as_deref(), Some("cache"));
}

/// The walk and the removal of a large cache at the end of its job run
/// off the tick: the tick answers the questions of the units and keeps
/// the limits.
#[test]
fn the_end_of_a_large_cache_does_not_hold_the_tick() {
    let n = 40_000;
    let cfg = config("[cache]\nmax_bytes = 1\ntotal_bytes = 1");
    let script =
        format!("mkdir \"$PODRUNNER_CACHE/t\" && cd \"$PODRUNNER_CACHE/t\" && seq 1 {n} | xargs touch && echo x > x");
    let mut bed = one_job(cfg, &script);
    let mut longest = Duration::ZERO;
    let end = std::time::Instant::now() + Duration::from_secs(120);
    while !(bed.slots.jobs().count() == 1 && Bed::all_final(&bed.slots)) {
        assert!(std::time::Instant::now() < end, "{:?}", bed.states());
        let t = std::time::Instant::now();
        bed.slots.tick();
        longest = longest.max(t.elapsed());
        // The clock stays: the deadline of the start does not end a slow
        // write of the tree.
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(bed.slots.reports[0].exit_reason, podrunner_schema::ExitReason::Success);
    // The cost of one walk of a tree of the same size, for this host.
    let probe = TempDir::new();
    for i in 0..n {
        std::fs::File::create(probe.0.join(i.to_string())).unwrap();
    }
    let t = std::time::Instant::now();
    crate::disk::bytes_under(&probe.0);
    let walk = t.elapsed();
    assert!(longest < walk, "the longest tick {longest:?}, one walk {walk:?}");
    // The cache goes off the tick.
    let cache = bed.dir.0.join("state/cache");
    let gone = || std::fs::read_dir(&cache).unwrap().flatten().all(|e| e.file_name() == "lock");
    let wait = std::time::Instant::now() + Duration::from_secs(60);
    while !gone() && std::time::Instant::now() < wait {
        bed.slots.tick();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(gone(), "{:?}", std::fs::read_dir(&cache).unwrap().flatten().map(|e| e.file_name()).collect::<Vec<_>>());
}

/// Two `serve` processes on one `[cache] dir` would remove a cache that
/// the other one gives to a job.
#[test]
fn a_second_serve_on_one_cache_directory_is_refused() {
    let dir = TempDir::new();
    let cfg = || config(&format!("[cache]\ndir = \"{}\"", dir.0.join("cache").display()));
    let start = |state: &str| {
        Slots::start(cfg(), &dir.0.join(state), Bed::clock(), Box::new(|| NOW_UNIX), Vec::new(), Vec::new())
    };
    let first = start("a").expect("the first serve");
    let second = start("b").err().expect("the second serve is refused");
    assert!(second.to_string().contains("in use"), "{second}");
    drop(first);
    assert!(start("c").is_ok());
}

/// A push, then a pull request of the same repository, on a kind whose
/// capability names `modes`; the files that the pull request saw.
fn pull_request_on(modes: &[&str]) -> (String, String) {
    let dir = TempDir::new();
    let out = dir.0.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let cfg = config("[cache]");
    let mut cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    cap.mount_modes = modes.iter().map(|m| m.to_string()).collect();
    let pr = offer(
        30,
        "j-3",
        json!({ "trust_inputs": { "run_id": 3, "event": "pull_request", "ref": "refs/pull/7/merge" } }),
    );
    let push = offer(0, "j-1", json!({}));
    let pr_script = format!(
        r#"run=$(cut -d' ' -f2 .runner-root/.podrunner-job)
case $run in
1) echo push > "$PODRUNNER_CACHE/from-push" ;;
3) echo "${{PODRUNNER_CACHE_MODE:-none}}" > {out}/pr-mode ;;
esac
exit 0"#,
        out = out.display()
    );
    let mut bed = Bed::in_dir(
        cfg,
        vec![("t", "trusted", None, vec![push, pr])],
        vec![KindSpec { name: "fake", slots: 1, capability: cap, command: sh(&pr_script) }],
        dir,
        Bed::clock(),
    );
    bed.until(120, |s| s.jobs().count() == 2 && Bed::all_final(s));
    let read = |name: &str| std::fs::read_to_string(out.join(name)).unwrap_or_default();
    let audit = bed.slots.audit_lines().iter().filter(|a| a.job_id.as_str() == "j-3").filter_map(|a| a.detail.clone());
    (read("pr-mode"), audit.collect::<Vec<_>>().join(" "))
}

/// P2-C of the review of T-480: a kind that does not prove a `:ro` mount
/// read-only (podvm with no proven confine layer) gives a job that would
/// get `ro` no cache: its writes would reach the cache that the next push
/// to the default branch reads.
#[test]
fn a_pull_request_gets_no_cache_on_a_kind_with_no_proven_ro_mount() {
    let (mode, audit) = pull_request_on(&["rw"]);
    assert_eq!(mode, "none\n", "no cache on a kind with no proven ro mount");
    assert!(audit.contains("no-ro-mount"), "{audit}");
    let (mode, _) = pull_request_on(&["ro", "rw"]);
    assert_eq!(mode, "ro\n", "a proven ro mount");
}

/// P2-B of the review of T-480: a `rw` job whose count of the cache is
/// not back holds no slot of its kind: its unit is gone.
#[test]
fn a_rw_job_that_waits_for_its_count_holds_no_slot() {
    let n = 20_000;
    let script =
        format!("mkdir -p \"$PODRUNNER_CACHE/t\" && cd \"$PODRUNNER_CACHE/t\" && seq 1 {n} | xargs touch; exit 0");
    let mut bed = Bed::new(
        config("[cache]"),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({})), offer(0, "j-2", json!({}))])],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory),
            command: sh(&script),
        }],
    );
    let mut seen = false;
    let end = std::time::Instant::now() + Duration::from_secs(120);
    while !(bed.slots.jobs().count() == 2 && Bed::all_final(&bed.slots)) {
        assert!(std::time::Instant::now() < end, "{:?}", bed.states());
        bed.tick();
        if bed.slots.jobs().any(|j| j.state == crate::state::State::Destroyed) {
            seen = true;
            let next = bed.slots.jobs().any(|j| j.id.as_str() == "j-2" && j.unit.is_some());
            assert!(bed.slots.kind_views()[0].free == 1 || next, "{:?}", bed.states());
        }
    }
    assert!(seen, "no job waited for its count");
    for r in &bed.slots.reports {
        assert!(matches!(r.cache_written_bytes, Some(Residual::Count(_))), "{r:?}");
    }
}

/// P2-B of the second review of T-480: the report of a `rw` job waits
/// for the count of its cache at most `[deadlines] report_s`, then says
/// `unknown` with a reason; its scope stays in use until the count is
/// back, so no bound removes it.
#[test]
fn the_report_of_a_rw_job_waits_for_its_count_at_most_report_s() {
    let cfg = config("[deadlines]\nreport_s = 5\n[cache]");
    let mut bed = one_job(cfg, "echo x > \"$PODRUNNER_CACHE/f\"");
    let gate = bed.slots.cache_worker.as_ref().unwrap().gate.clone();
    let held = gate.lock().unwrap();
    bed.until(60, |s| s.jobs().any(|j| j.state == crate::state::State::Destroyed));
    let waited = bed.until(20, Bed::all_final);
    assert!(waited >= 4, "the report did not wait: {waited} ticks");
    let r = bed.slots.reports[0].clone();
    assert!(matches!(r.cache_written_bytes, Some(Residual::Unknown(_))), "{r:?}");
    assert!(r.unknown_reasons.get("cache_written_bytes").is_some_and(|w| w.contains("report_s")), "{r:?}");
    let id = bed.slots.jobs().next().unwrap().times.cache.clone().unwrap().id;
    bed.tick();
    let w = bed.slots.cache_worker.as_ref().unwrap();
    assert_eq!(w.uses().holders(&id), 1, "the scope is in use until its count is back");
    drop(held);
    bed.until(30, |s| s.cache_worker.as_ref().is_some_and(|w| w.ending.is_empty()));
    bed.tick();
    assert_eq!(bed.slots.cache_worker.as_ref().unwrap().uses().holders(&id), 0);
}
