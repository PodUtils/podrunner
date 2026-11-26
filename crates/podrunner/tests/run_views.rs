//! The views of `podrunner run` (`--list`, `--graph`, `--validate`) out of
//! a git tree: they check out nothing, so they need no `--checkout` (T-634).

use scratch::Scratch;

mod scratch;

#[test]
fn a_view_of_a_workflow_with_checkout_needs_no_tree() {
    let dir = Scratch::new("run-views");
    let flow = "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{uses: actions/checkout@v4}]\n";
    std::fs::write(dir.join("w.yml"), flow).unwrap();
    let parent = dir.parent().unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_podrunner"))
            .current_dir(&*dir)
            .args(["run", "w.yml", "--no-config"])
            .args(args)
            // git finds no tree above the scratch directory.
            .env("GIT_CEILING_DIRECTORIES", parent)
            .env("XDG_CONFIG_HOME", dir.join("config"))
            .env("XDG_CACHE_HOME", dir.join("cache"))
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    for view in ["--validate", "--list", "--graph"] {
        let out = run(&[view]);
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(0), "{view}: {err}");
        assert!(!err.contains("--checkout"), "{view}: {err}");
    }
    // A run of the jobs still needs the tree, and says so.
    let out = run(&["--runner-dir", "/nonexistent", "--dry-run", "--no-cache"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(64), "{err}");
    assert!(err.contains("is in no git tree: give --checkout DIR"), "{err}");
}
