//! `podrunner run` runs the suite of the local mode to green on the official
//! runner (T-089), with the tree of this repository for `actions/checkout`
//! (T-092). Ignored by default: it needs an unpacked release of
//! `actions/runner` in a directory it may change.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner --test local -- --ignored

#[test]
#[ignore]
fn podrunner_run_runs_the_local_suite_to_green() {
    let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR");
    let suite = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/local/suite.yml");
    let tree = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let out = podrunner::cli::run(&[
        "run".into(),
        suite.into(),
        "--runner-dir".into(),
        dir,
        "--checkout".into(),
        tree.into(),
    ]);
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
        "ok      checkout",
        "skipped skipped: if: success() && (github.event_name == 'pull_request')",
        "ok      after-skip",
    ] {
        assert!(lines.contains(&want), "{want:?} in\n{}", out.stdout);
    }
    assert_eq!(lines.len(), 9, "{}", out.stdout);
}

/// T-133: with `--with-git` a step reads HEAD of the tree, and a checkout of
/// another repository takes its tree from the store at its pinned commit.
/// It needs github.com for the second tree.
#[test]
#[ignore]
fn the_checkout_gives_git_and_another_repository() {
    let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR");
    let scratch = std::env::temp_dir().join(format!("podrunner-local-checkout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let flow = scratch.join("checkout.yml");
    std::fs::write(
        &flow,
        "on: push\njobs:\n  git:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n      \
         - run: test \"$(git rev-parse HEAD)\" = \"${{ github.sha }}\" && test -f Cargo.toml\n      \
         - uses: actions/checkout@v4\n        with:\n          repository: actions/upload-pages-artifact\n          \
         ref: v3\n          path: other\n      - run: test -f other/action.yml && test ! -f other/Cargo.toml\n",
    )
    .unwrap();
    let tree = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let args: Vec<String> = [
        "run",
        flow.to_str().unwrap(),
        "--runner-dir",
        &dir,
        "--checkout",
        tree,
        "--with-git",
        "--actions",
        scratch.join("store").to_str().unwrap(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let out = podrunner::cli::run(&args);
    println!("{}{}", out.stdout, out.stderr);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.lines().any(|l| l == "ok      git"), "{}", out.stdout);
    let lock = std::fs::read_to_string(scratch.join("podrunner-actions.lock")).unwrap();
    assert!(lock.contains("actions/upload-pages-artifact@v3 "), "the tree is pinned: {lock}");
    let _ = std::fs::remove_dir_all(&scratch);
}
