//! A run of the podrunner binary from a subdirectory of a git tree. It has
//! a test binary of its own: a run in the process of `tests/local.rs`
//! holds the lock of the runner directory until that process ends, so a
//! child process there cannot take it.

/// T-421: with no `--checkout`, a run from a subdirectory of a git tree
/// reads a called workflow from the top of the tree, and the checkout of
/// that called workflow gets the tree.
#[test]
#[ignore]
fn a_checkout_in_a_called_workflow_gets_the_tree() {
    let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR");
    let scratch = std::env::temp_dir().join(format!("podrunner-local-called-checkout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(scratch.join(".github/workflows")).unwrap();
    std::fs::create_dir_all(scratch.join("sub")).unwrap();
    std::fs::write(scratch.join("marker.txt"), "tree\n").unwrap();
    std::fs::write(
        scratch.join(".github/workflows/main.yml"),
        "on: push\njobs:\n  call:\n    uses: ./.github/workflows/build.yml\n",
    )
    .unwrap();
    std::fs::write(
        scratch.join(".github/workflows/build.yml"),
        "on: workflow_call\njobs:\n  b:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n      \
         - run: test -f marker.txt\n",
    )
    .unwrap();
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git").arg("-C").arg(&scratch).args(args).status().unwrap().success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["add", "-A"]);
    git(&["-c", "user.name=t", "-c", "user.email=t@example.invalid", "commit", "-qm", "t"]);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_podrunner"))
        .current_dir(scratch.join("sub"))
        .args(["run", "../.github/workflows/main.yml", "--runner-dir", &dir, "-q", "--actions"])
        .arg(scratch.join("store"))
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    println!("{stdout}{stderr}");
    assert!(out.status.success(), "{stdout}{stderr}");
    assert!(stdout.lines().any(|l| l.starts_with("ok") && l.contains("call / b")), "{stdout}");
    let _ = std::fs::remove_dir_all(&scratch);
}
