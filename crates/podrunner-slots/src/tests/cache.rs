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
   echo pr > "$PODRUNNER_CACHE/from-pr" ;;
4) ls -A "$PODRUNNER_CACHE" > {out}/last ;;
esac
exit 0"#
    )
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
            capability: capability(Boundary::Guest, Principal::Distinct, EgressHold::Enforced),
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
