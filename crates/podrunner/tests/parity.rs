//! The parity suite of the local mode (T-147): `podrunner run` on each
//! workflow of `tests/local/parity`, with one verdict for each. A failure
//! is PARTIAL only while the entry that closes it is open in the record,
//! and only in the jobs that need that entry; after that entry is done, the
//! same failure is FAIL. A plant (a wrong value of a check) gives FAIL.
//! The run of the suite is ignored by default; the checks of its text are
//! not.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner --test parity -- --ignored --nocapture
//!
//! DIR is an unpacked release of `actions/runner` that the test may change.
//! With `--runner-dir` a workflow that needs a container route gives SKIP.
//! `PODRUNNER_PARITY_UNIT` replaces `--runner-dir DIR` with the options of a
//! unit kind, split at each space (for example `--microvm PODVM --images
//! STORE --image REF --engine DIR --allow HOST`), and each workflow runs;
//! with `PODRUNNER_PARITY_ROUTES=0` too, a workflow that needs a container
//! route gives SKIP.
//! The actions come from github.com by the lock of the suite, into the
//! store of the user.

use std::path::{Path, PathBuf};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const SUITE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/local/parity");

/// The open entry that a failure waits for, and the jobs that need it (no
/// job named: each job of the workflow).
type Gap = Option<(&'static str, &'static [&'static str])>;

/// Each workflow, its gap, and whether it needs a container route.
const ROWS: [(&str, Gap, bool); 11] = [
    ("01-exit.yml", None, false),
    ("02-expressions.yml", None, false),
    ("03-matrix.yml", None, false),
    ("04-services.yml", None, true),
    ("05-containers.yml", None, true),
    ("06-actions.yml", None, false),
    ("07-artifacts.yml", None, false),
    ("08-cache.yml", None, false),
    ("09-dockerfile.yml", None, true),
    ("10-docker-build.yml", None, true),
    ("11-docker-dind.yml", None, true),
];

/// The jobs of the suite that `level2.yml` runs on github.com (T-516):
/// the file, its job, and the job of `level2.yml`.
const HOSTED: [(&str, &str, &str); 2] =
    [("10-docker-build.yml", "build", "docker-build"), ("11-docker-dind.yml", "dind", "docker-dind")];

#[derive(Debug, PartialEq)]
enum Verdict {
    Pass,
    Partial(&'static str),
    Fail,
}

/// The status of an entry in the record: the `**Status:**` line under its
/// heading.
fn status(entry: &str) -> String {
    let head = format!("# {entry}:");
    for file in std::fs::read_dir(Path::new(ROOT).join("TODO")).unwrap().flatten() {
        let text = std::fs::read_to_string(file.path()).unwrap_or_default();
        let Some(at) = text.lines().position(|l| l.starts_with(&head)) else { continue };
        if let Some(s) = text.lines().skip(at).find_map(|l| l.strip_prefix("**Status:** ")) {
            return s.trim().to_string();
        }
    }
    panic!("{entry} is not in the record")
}

/// A copy of the workflow and of the lock of the actions of the suite
/// (T-118) in `scratch`.
fn workflow(name: &str, scratch: &Path) -> PathBuf {
    let lock = podrunner_local::lock::LOCK_FILE;
    std::fs::copy(Path::new(SUITE).join(lock), scratch.join(lock)).unwrap();
    let path = scratch.join(name);
    std::fs::copy(Path::new(SUITE).join(name), &path).unwrap();
    path
}

/// The jobs that failed: the `failure` lines of the output. A job that a
/// failure skipped or cancelled follows from that failure.
fn failed(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|l| l.strip_prefix("failure "))
        .map(|rest| rest.split(": ").next().unwrap_or(rest).trim().to_string())
        .collect()
}

fn verdict(unit: &[String], scratch: &Path, name: &str, gap: Gap, extra: &[&str]) -> Verdict {
    let mut args: Vec<String> = vec!["run".into(), workflow(name, scratch).display().to_string()];
    args.extend(unit.iter().cloned());
    // The lock of the suite pins each action: a run may not change it.
    args.extend(["--checkout".into(), ROOT.into(), "--frozen".into()]);
    args.extend(extra.iter().map(|s| s.to_string()));
    let out = podrunner::cli::run(&args);
    println!("--- {name}\n{}{}", out.stdout, out.stderr);
    if out.code == 0 {
        return Verdict::Pass;
    }
    let failed = failed(&out.stdout);
    match gap {
        Some((entry, jobs))
            if status(entry) != "done"
                && !failed.is_empty()
                && failed.iter().all(|j| jobs.is_empty() || jobs.contains(&j.as_str())) =>
        {
            Verdict::Partial(entry)
        }
        _ => Verdict::Fail,
    }
}

#[test]
#[ignore]
fn the_parity_suite_gives_one_verdict_for_each_workflow() {
    let (unit, routes) = match std::env::var("PODRUNNER_PARITY_UNIT") {
        // `PODRUNNER_PARITY_ROUTES=0`: a unit with no container route (the
        // incus kind with no engine, T-481) skips those workflows.
        Ok(u) => (
            u.split(' ').filter(|w| !w.is_empty()).map(str::to_string).collect::<Vec<_>>(),
            std::env::var("PODRUNNER_PARITY_ROUTES").as_deref() != Ok("0"),
        ),
        Err(_) => {
            let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR or PODRUNNER_PARITY_UNIT");
            (vec!["--runner-dir".to_string(), dir], false)
        }
    };
    let scratch = std::env::temp_dir().join(format!("podrunner-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let mut bad = Vec::new();
    for (name, gap, needs_route) in ROWS {
        if needs_route && !routes {
            println!("parity {name} SKIP: it needs a unit with a container route");
            continue;
        }
        let v = verdict(&unit, &scratch, name, gap, &[]);
        let word = match &v {
            Verdict::Pass => "PASS".to_string(),
            Verdict::Partial(e) => format!("PARTIAL {e}"),
            Verdict::Fail => "FAIL".to_string(),
        };
        println!("parity {name} {word}");
        if v == Verdict::Fail {
            bad.push(name);
        }
    }
    // The plants: a wrong value of an expression, a wrong artifact content.
    for name in ["02-expressions.yml", "07-artifacts.yml"] {
        let plant = verdict(&unit, &scratch, name, None, &["--var", "PLANT=1"]);
        println!("parity plant {name} {plant:?}");
        assert_eq!(plant, Verdict::Fail, "the plant of {name} must fail");
    }
    let _ = std::fs::remove_dir_all(&scratch);
    assert!(bad.is_empty(), "FAIL: {bad:?}");
}

/// The lines of a job of a workflow (two spaces of indent under `jobs:`),
/// with no `runs-on` and no `timeout-minutes`: the two keys that differ on
/// github.com.
fn job_body(text: &str, job: &str) -> Vec<String> {
    let head = format!("  {job}:");
    let at = text.lines().position(|l| l == head).unwrap_or_else(|| panic!("no job {job}"));
    let mut body: Vec<String> = text
        .lines()
        .skip(at + 1)
        .take_while(|l| l.is_empty() || l.starts_with("   "))
        .filter(|l| !["runs-on:", "timeout-minutes:"].iter().any(|k| l.trim_start().starts_with(k)))
        .map(str::to_string)
        .collect();
    while body.last().is_some_and(|l| l.is_empty()) {
        body.pop();
    }
    body
}

/// T-516: the github.com side of the parity is the same text. A change of
/// a workflow of the suite that `level2.yml` does not follow fails here.
#[test]
fn the_hosted_jobs_of_level2_are_the_jobs_of_the_suite() {
    let hosted = std::fs::read_to_string(Path::new(ROOT).join(".github/workflows/level2.yml")).unwrap();
    for (name, job, twin) in HOSTED {
        let suite = std::fs::read_to_string(Path::new(SUITE).join(name)).unwrap();
        let ours = job_body(&suite, job);
        assert!(ours.iter().any(|l| l.trim_start().starts_with("- run:")), "{name}: no step");
        assert_eq!(ours, job_body(&hosted, twin), "{name} job {job} and level2.yml job {twin}");
    }
}

/// Each workflow of the suite has a row, and each one is valid for
/// `podrunner run --validate` with no warning (no unit, no network).
#[test]
fn each_workflow_of_the_suite_has_a_row_and_is_valid() {
    let mut files: Vec<String> = std::fs::read_dir(SUITE)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".yml"))
        .collect();
    files.sort();
    let rows: Vec<String> = ROWS.iter().map(|r| r.0.to_string()).collect();
    assert_eq!(files, rows, "the rows of the suite");
    for name in rows {
        let path = Path::new(SUITE).join(&name).display().to_string();
        let out = podrunner::cli::run(&["run".into(), path, "--validate".into(), "--no-config".into()]);
        assert_eq!(out.code, 0, "{name}: {}{}", out.stdout, out.stderr);
        // An unknown key is a warning only: here it is a defect of the suite.
        assert!(!out.stdout.contains("warning:"), "{name}: {}", out.stdout);
    }
}
