//! `podrunner run` runs the suite of the local mode to green on the official
//! runner (T-089). Ignored by default: it needs an unpacked release of
//! `actions/runner` in a directory it may change.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner --test local -- --ignored

#[test]
#[ignore]
fn podrunner_run_runs_the_local_suite_to_green() {
    let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR");
    let suite = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/local/suite.yml");
    let out = podrunner::cli::run(&["run".into(), suite.into(), "--runner-dir".into(), dir]);
    println!("{}{}", out.stdout, out.stderr);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let lines: Vec<&str> = out.stdout.lines().collect();
    for want in [
        "ok      matrix (alpha, 1)",
        "ok      matrix (alpha, 2)",
        "ok      matrix (beta, 1)",
        "ok      matrix (gamma, 3)",
        "ok      needs",
        "ok      steps",
        "skipped skipped: if: success() && (github.event_name == 'pull_request')",
        "ok      after-skip",
    ] {
        assert!(lines.contains(&want), "{want:?} in\n{}", out.stdout);
    }
    assert_eq!(lines.len(), 8, "{}", out.stdout);
}
