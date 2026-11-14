//! The parity suite of the local mode (T-147): `podrunner run` on each
//! workflow of `tests/local/parity`, with one verdict for each. A failure
//! is PARTIAL only while the entry that closes it is open in the record,
//! and only in the jobs that need that entry; after that entry is done, the
//! same failure is FAIL. A plant (a wrong
//! value of a check) gives FAIL. Ignored by default: it needs an unpacked
//! release of `actions/runner` in a directory it may change.
//!
//!   PODRUNNER_RUNNER_DIR=DIR [PODRUNNER_PARITY_IMAGE=NAME@sha256:HEX] \
//!   cargo test -p podrunner --test parity -- --ignored --nocapture
//!
//! PODRUNNER_PARITY_IMAGE is an image with busybox for the workflows with
//! containers; with no value, those workflows keep the word IMAGE and fail.
//! The actions come from github.com by the lock of the suite, into the
//! store of the user.

use std::path::{Path, PathBuf};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const SUITE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/local/parity");

/// The open entry that a failure waits for, and the jobs that need it (no
/// job named: each job of the workflow).
type Gap = Option<(&'static str, &'static [&'static str])>;

/// Each workflow and its gap.
const ROWS: [(&str, Gap); 8] = [
    ("01-exit.yml", None),
    ("02-expressions.yml", None),
    ("03-matrix.yml", None),
    ("04-services.yml", Some(("T-124", &[]))),
    ("05-containers.yml", Some(("T-123", &[]))),
    ("06-actions.yml", None),
    ("07-artifacts.yml", None),
    ("08-cache.yml", None),
];

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

/// A copy of the workflow with IMAGE replaced, and of the lock of the
/// actions of the suite (T-118), in `scratch`.
fn workflow(name: &str, scratch: &Path) -> PathBuf {
    let lock = podrunner_local::lock::LOCK_FILE;
    std::fs::copy(Path::new(SUITE).join(lock), scratch.join(lock)).unwrap();
    let text = std::fs::read_to_string(Path::new(SUITE).join(name)).unwrap();
    let text = match std::env::var("PODRUNNER_PARITY_IMAGE") {
        Ok(image) => text.replace("IMAGE", &image),
        Err(_) => text,
    };
    let path = scratch.join(name);
    std::fs::write(&path, text).unwrap();
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

fn verdict(dir: &str, scratch: &Path, name: &str, gap: Gap, extra: &[&str]) -> Verdict {
    let mut args: Vec<String> = vec![
        "run".into(),
        workflow(name, scratch).display().to_string(),
        "--runner-dir".into(),
        dir.into(),
        "--checkout".into(),
        ROOT.into(),
        // The lock of the suite pins each action: a run may not change it.
        "--frozen".into(),
    ];
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
    let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR");
    let scratch = std::env::temp_dir().join(format!("podrunner-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let mut bad = Vec::new();
    for (name, gap) in ROWS {
        let v = verdict(&dir, &scratch, name, gap, &[]);
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
        let plant = verdict(&dir, &scratch, name, None, &["--var", "PLANT=1"]);
        println!("parity plant {name} {plant:?}");
        assert_eq!(plant, Verdict::Fail, "the plant of {name} must fail");
    }
    let _ = std::fs::remove_dir_all(&scratch);
    assert!(bad.is_empty(), "FAIL: {bad:?}");
}
