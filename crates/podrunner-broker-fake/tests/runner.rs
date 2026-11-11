//! The official runner runs canned jobs against the fake broker, offline.
//! Ignored by default: it needs an unpacked release of `actions/runner`
//! (v2.338.2, `images/runner/runner.toml`) in a directory it may change,
//! and openssl for the key of the test.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-broker-fake --test runner -- --ignored

use std::sync::Mutex;
use std::time::Duration;

use podrunner_broker_fake::{Broker, Job};

mod common;

use common::run_once;

/// The tests share one runner directory and its configuration: one at a
/// time.
static DIR: Mutex<()> = Mutex::new(());

#[test]
#[ignore]
fn the_official_runner_runs_a_canned_job_offline() {
    let _one = DIR.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::path::PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let broker = Broker::start().unwrap();
    // A computed marker: the echo of the script cannot match it.
    let ok = Job::scripts("hello", &["echo podrunner-step-$((6*7)) | tee marker.txt"]);
    broker.queue(ok);
    let _ = std::fs::remove_dir_all(dir.join("_work"));
    assert!(run_once(&dir, &broker, &[]).success());
    let done = broker.completions();
    assert_eq!(done.len(), 1, "{:?}", broker.seen());
    assert_eq!(done[0].conclusion, "succeeded");
    let marker = std::fs::read_to_string(dir.join("_work/app/app/marker.txt")).unwrap();
    assert_eq!(marker.trim(), "podrunner-step-42");
    // The output of the step reaches the results service as a log.
    assert!(broker.logs().contains("podrunner-step-42"), "{:?}", broker.seen());
    assert!(!broker.step_updates().is_empty());

    // A step that fails fails the job, and a later step does not run.
    let broker = Broker::start().unwrap();
    broker.queue(Job::scripts("fails", &["exit 3", "echo no > second.txt"]));
    let _ = std::fs::remove_dir_all(dir.join("_work"));
    run_once(&dir, &broker, &[]);
    std::thread::sleep(Duration::from_millis(100));
    let done = broker.completions();
    assert_eq!(done[0].conclusion, "failed", "{done:?}");
    assert!(!dir.join("_work/app/app/second.txt").exists());
}

/// T-066: a "version too old" answer of the broker gives exit 7 with the
/// variable of the start environment of slots, and another code with no
/// variable (the plant).
#[test]
#[ignore]
fn version_too_old_gives_exit_7_with_the_variable() {
    let _one = DIR.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::path::PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let broker = Broker::start().unwrap();
    broker.refuse_the_version();
    let with = run_once(&dir, &broker, &[("ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE", "1".into())]);
    assert_eq!(with.code(), Some(7), "{:?}", broker.seen());
    let without = run_once(&dir, &broker, &[]);
    assert_ne!(without.code(), Some(7));
}
