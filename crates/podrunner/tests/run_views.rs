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

/// A dry run goes through the start path of a run. It prints the warning of
/// `--validate` once on stderr for a job above 7200 minutes, and none for
/// 7200 or for an expression (T-709). The warning goes to the live stderr,
/// so the test runs the binary.
#[test]
fn a_run_warns_on_a_timeout_above_five_days() {
    let dir = Scratch::new("run-timeout");
    std::fs::create_dir_all(dir.join("runner/bin")).unwrap();
    std::fs::write(dir.join("runner/run.sh"), "").unwrap();
    std::fs::write(dir.join("runner/bin/Runner.Listener"), "").unwrap();
    let go = |minutes: &str| {
        let flow =
            format!("on: push\njobs:\n  a:\n    runs-on: x\n    timeout-minutes: {minutes}\n    steps: [{{run: a}}]\n");
        std::fs::write(dir.join("w.yml"), flow).unwrap();
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_podrunner"))
            .current_dir(&*dir)
            .args(["run", "w.yml", "--no-config", "--dry-run", "--no-cache", "--checkout", "."])
            .arg("--runner-dir")
            .arg(dir.join("runner"))
            .env("XDG_CONFIG_HOME", dir.join("config"))
            .env("XDG_CACHE_HOME", dir.join("cache"))
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        (out.status.code(), String::from_utf8_lossy(&out.stderr).into_owned())
    };
    let (code, err) = go("99999999");
    assert_eq!(code, Some(0), "{err}");
    let key = "warning: jobs.a.timeout-minutes (line 5): 99999999 is more than 7200";
    assert_eq!(err.matches(key).count(), 1, "{err}");
    for ok in ["7200", "'${{ 5 }}'"] {
        let (code, err) = go(ok);
        assert_eq!(code, Some(0), "{ok}: {err}");
        assert!(!err.contains("timeout-minutes"), "{ok}: {err}");
    }
}

/// T-308 A23: a run that its start refuses (a runner directory that another
/// run holds) prints the refusal and no line of the egress hold.
#[test]
fn a_refused_start_prints_no_line_of_the_hold() {
    let dir = Scratch::new("run-refused-hold");
    std::fs::create_dir_all(dir.join("runner/bin")).unwrap();
    std::fs::write(dir.join("runner/run.sh"), "").unwrap();
    std::fs::write(dir.join("runner/bin/Runner.Listener"), "").unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    let lock = dir.join("runner/.podrunner-run.lock");
    let held = pod_core::lock::FileLock::try_lock(&lock, pod_core::lock::Mode::Exclusive).unwrap().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_podrunner"))
        .current_dir(&*dir)
        .args(["run", "w.yml", "--no-config", "--no-cache", "--checkout", ".", "--runner-dir", "runner"])
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    drop(held);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_ne!(out.status.code(), Some(0), "{err}");
    assert!(err.contains("another run of podrunner uses this runner directory"), "{err}");
    assert!(!err.contains("the egress hold"), "{err}");
}
