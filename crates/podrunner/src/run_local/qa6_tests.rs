//! The checks of a user QA of `v0.1.0-beta.7` (GitHub #203 to #212).

use std::path::PathBuf;

use super::*;

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// A new directory for one test.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("podrunner-qa6-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// An unpacked runner for a dry run, which checks the runner directory
/// (T-345).
pub(super) fn runner_dir(tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("podrunner-qa6-runner-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("run.sh"), "").unwrap();
    std::fs::write(dir.join("bin/Runner.Listener"), "").unwrap();
    dir.display().to_string()
}

const WORKFLOW: &str = "on: push\njobs:\n  a:\n    runs-on: ubuntu-latest\n    steps: [{run: a}]\n";

/// T-340: a workflow file whose link leads out of its project is refused,
/// in file mode and in directory mode; a link in the project is taken.
#[test]
fn a_workflow_link_out_of_the_project_is_refused() {
    let dir = scratch("link");
    let flows = dir.join("p/.github/workflows");
    std::fs::create_dir_all(&flows).unwrap();
    std::fs::create_dir_all(dir.join("other")).unwrap();
    std::fs::write(dir.join("other/w.yml"), WORKFLOW).unwrap();
    std::fs::write(flows.join("in.yml"), WORKFLOW).unwrap();
    std::os::unix::fs::symlink(flows.join("in.yml"), flows.join("ok.yml")).unwrap();
    let ok = flows.join("ok.yml").display().to_string();
    let out = run(&args(&[&ok, "--runner-dir", "/r", "--validate"]));
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    std::os::unix::fs::symlink(dir.join("other/w.yml"), flows.join("out.yml")).unwrap();
    let out_file = flows.join("out.yml").display().to_string();
    let flows_dir = flows.display().to_string();
    for a in [
        vec![out_file.as_str(), "--validate"],
        vec![out_file.as_str(), "--dry-run"],
        vec![flows_dir.as_str(), "--list"],
    ] {
        let mut a = a.clone();
        a.extend(["--runner-dir", "/r"]);
        let out = run(&args(&a));
        assert_ne!(out.code, 0, "{a:?}: {}", out.stdout);
        assert!(out.stderr.contains("the symbolic link leads out of the project"), "{a:?}: {}", out.stderr);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-341: `.secrets` of the tree gets each check of `--secret-file` in a
/// dry run: the mode, a bad line, a directory and a link out of the tree.
#[test]
fn the_secrets_of_the_tree_are_checked_in_a_dry_run() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("secrets");
    let tree = dir.join("tree");
    std::fs::create_dir_all(&tree).unwrap();
    std::fs::write(dir.join("w.yml"), WORKFLOW).unwrap();
    std::fs::write(dir.join("outside"), "A=1\n").unwrap();
    let w = dir.join("w.yml").display().to_string();
    let t = tree.display().to_string();
    let secrets = tree.join(".secrets");
    let r = runner_dir("secrets");
    let dry = || run(&args(&[&w, "--runner-dir", &r, "--dry-run", "--no-cache", "--checkout", &t]));
    let set = |text: &str, mode: u32| {
        std::fs::write(&secrets, text).unwrap();
        std::fs::set_permissions(&secrets, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    set("A=1\n", 0o600);
    let out = dry();
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    for (why, make) in [
        ("needs mode 0600", Box::new(|| set("A=1\n", 0o644)) as Box<dyn Fn()>),
        ("not NAME=VALUE", Box::new(|| set("BADLINE\n", 0o600))),
        (
            "not a file",
            Box::new(|| {
                std::fs::remove_file(&secrets).unwrap();
                std::fs::create_dir(&secrets).unwrap()
            }),
        ),
        (
            "leads out of the tree",
            Box::new(|| {
                std::fs::remove_dir(&secrets).unwrap();
                std::os::unix::fs::symlink(dir.join("outside"), &secrets).unwrap()
            }),
        ),
    ] {
        make();
        let out = dry();
        assert_eq!(out.code, 78, "{why}: {}{}", out.stdout, out.stderr);
        assert!(out.stderr.contains(".secrets") && out.stderr.contains(why), "{why}: {}", out.stderr);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-346: with `--offline`, a dry run fails on a reference that a local
/// composite action names and the lock does not pin, and names the fix.
#[test]
fn an_offline_dry_run_pins_the_uses_of_a_local_composite() {
    let dir = scratch("composite");
    let tree = dir.join("tree");
    let action = tree.join(".github/actions/nested");
    std::fs::create_dir_all(&action).unwrap();
    std::fs::write(
        action.join("action.yml"),
        "runs:\n  using: composite\n  steps:\n    - uses: actions/setup-node@v4\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("w.yml"),
        "on: push\njobs:\n  a:\n    runs-on: ubuntu-latest\n    steps: [{uses: ./.github/actions/nested}]\n",
    )
    .unwrap();
    let (w, t, store) =
        (dir.join("w.yml").display().to_string(), tree.display().to_string(), dir.join("store").display().to_string());
    let out = run(&args(&[
        &w,
        "--runner-dir",
        "/r",
        "--dry-run",
        "--no-cache",
        "--offline",
        "--checkout",
        &t,
        "--actions",
        &store,
    ]));
    assert_eq!(out.code, 78, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stderr.contains("actions/setup-node@v4") && out.stderr.contains("run with no --offline"),
        "{}",
        out.stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-348: `--validate --json` writes an object for a file that is not valid.
#[test]
fn validate_json_names_a_file_that_is_not_valid() {
    let dir = scratch("validate");
    std::fs::write(dir.join("bad.yml"), "on: push\njobs: [\n").unwrap();
    let w = dir.join("bad.yml").display().to_string();
    let out = run(&args(&[&w, "--validate", "--json"]));
    assert_eq!(out.code, 78, "{}", out.stderr);
    let v: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
    assert_eq!(v["valid"], false);
    assert_eq!(v["file"], w.as_str());
    assert!(v["error"].as_str().is_some_and(|e| !e.is_empty()), "{v}");
    assert!(out.stderr.starts_with("podrunner: "), "{}", out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-347: a loop with a view that runs once is refused, and `--schedule` of
/// a workflow with no `on.schedule` says so.
#[test]
fn a_loop_refuses_a_view_and_a_workflow_with_no_schedule() {
    let dir = scratch("loops");
    std::fs::write(dir.join("w.yml"), WORKFLOW).unwrap();
    let w = dir.join("w.yml").display().to_string();
    for (lp, view) in
        [("--watch", "--list"), ("--schedule", "--dry-run"), ("--watch", "--validate"), ("--schedule", "--graph")]
    {
        let out = repeat(&args(&[&w, "--runner-dir", "/r", lp, view]));
        assert_eq!(out.code, 64, "{lp} {view}: {}", out.stderr);
        assert!(out.stderr.contains(&format!("{lp} and {view} exclude each other")), "{}", out.stderr);
    }
    let out = repeat(&args(&[&w, "--runner-dir", "/r", "--schedule"]));
    assert_eq!(out.code, 78, "{}", out.stderr);
    assert!(out.stderr.contains("the workflow has no on.schedule trigger"), "{}", out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-349: a job that failed before it ran gets its cause as `job.log` in
/// the directory of the logs, and the end line names a directory only
/// when the run wrote one.
#[test]
fn a_failure_before_the_start_has_its_log_in_the_log_dir() {
    let dir = scratch("logdir");
    let job = podrunner_schema::JobId::new("lint.0").unwrap();
    let logs = vec![(job.clone(), "the fake broker: bind 127.0.0.1:0: Permission denied".to_string())];
    logs::write_dir(&dir, &[], &logs, &[]).unwrap();
    let text = std::fs::read_to_string(dir.join("lint.0/job.log")).unwrap();
    assert!(text.contains("Permission denied"), "{text}");
    let end = logs::failed_end(&logs, false, false, false, None);
    assert_eq!(end, "podrunner: a job failed; its log is above\n");
    let end = logs::failed_end(&logs, false, false, false, Some(&dir));
    assert!(end.contains(&dir.display().to_string()), "{end}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-339 item 17: two executors with no store name the clash, not the store.
#[test]
fn two_executors_name_the_clash_first() {
    let e = parse(&args(&["w.yml", "--podvm", "/p", "--runner-dir", "/r"])).unwrap_err();
    assert!(e.contains("two executors"), "{e}");
}

/// T-331: a wrong value names its flag with no usage; a wrong form of the
/// line gives the usage.
#[test]
fn the_usage_comes_with_a_wrong_form_only() {
    let out = run(&args(&["w.yml", "--runner-dir", "/r", "--event", "nope"]));
    assert!(out.stderr.contains("--event nope") && !out.stderr.contains("usage:"), "{}", out.stderr);
    let out = run(&args(&["w.yml", "--no-such-flag"]));
    assert!(out.stderr.contains("usage:"), "{}", out.stderr);
}

/// T-323 F20, F24: `--allow "*"` and `--web` with `--dry-run` are refused.
#[test]
fn a_wildcard_allow_and_a_web_dry_run_are_refused() {
    let pin = format!("i@sha256:{}", "a".repeat(64));
    let e =
        parse(&args(&["w.yml", "--podvm", "/p", "--images", "/s", "--image", &pin, "--podnet", "/n", "--allow", "*"]))
            .unwrap_err();
    assert!(e.contains("--allow \"*\""), "{e}");
    let e = parse(&args(&["w.yml", "--runner-dir", "/r", "--dry-run", "--web", "127.0.0.1:0"])).unwrap_err();
    assert!(e.contains("--web and --dry-run"), "{e}");
}

/// T-356: a real run refuses a missing `--toolcache` before its first unit.
#[test]
fn a_run_refuses_a_missing_toolcache_before_a_unit() {
    let dir = scratch("toolcache");
    std::fs::write(dir.join("w.yml"), WORKFLOW).unwrap();
    let w = dir.join("w.yml").display().to_string();
    let pin = format!("i@sha256:{}", "a".repeat(64));
    let none = dir.join("none").display().to_string();
    let out = run(&args(&[
        &w,
        "--podvm",
        "/bin/true",
        "--images",
        "/s",
        "--image",
        &pin,
        "--toolcache",
        &none,
        "--no-cache",
    ]));
    assert_eq!(out.code, 78, "{}{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("--toolcache") && out.stderr.contains("mkdir -p"), "{}", out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The command line of `run` as a contract (T-245).
#[path = "contract_tests.rs"]
mod contract;

/// #226: `run DIR --checkout DIR` keeps the value of `--checkout`.
#[test]
fn a_checkout_with_the_text_of_the_directory_stays() {
    let dir = scratch("dircheckout");
    std::fs::write(dir.join("a.yml"), WORKFLOW).unwrap();
    let d = dir.display().to_string();
    let r = runner_dir("dircheckout");
    for a in [vec!["--checkout", d.as_str(), d.as_str()], vec![d.as_str(), "--checkout", d.as_str()]] {
        let mut a = a.clone();
        a.extend(["--runner-dir", r.as_str(), "--dry-run", "--no-cache"]);
        let out = run(&args(&a));
        assert_eq!(out.code, 0, "{a:?}: {}{}", out.stdout, out.stderr);
        assert!(out.stdout.contains("planned a"), "{}", out.stdout);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// #229: `--github-token-env ""` sends no token.
#[test]
fn an_empty_token_variable_sends_no_token() {
    let a = parse(&args(&["w.yml", "--runner-dir", "/r", "--github-token-env", ""])).unwrap();
    let (_, token) = super::secrets::secrets(&a).unwrap();
    assert_eq!(token, None);
}

/// #230: a pull request is filtered on the base branch of its payload.
#[test]
fn a_pull_request_is_filtered_on_its_base_branch() {
    let dir = scratch("prbase");
    std::fs::write(
        dir.join("w.yml"),
        "on:\n  pull_request:\n    branches: ['release/*']\njobs:\n  a:\n    runs-on: ubuntu-latest\n    steps: [{run: a}]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("p.json"),
        r#"{"action":"opened","pull_request":{"base":{"ref":"release/1"},"head":{"ref":"f"}}}"#,
    )
    .unwrap();
    let (w, p) = (dir.join("w.yml").display().to_string(), dir.join("p.json").display().to_string());
    let r = runner_dir("prbase");
    let out =
        run(&args(&[&w, "--runner-dir", &r, "--dry-run", "--no-cache", "--event", "pull_request", "--event-path", &p]));
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    std::fs::write(
        dir.join("p.json"),
        r#"{"action":"opened","pull_request":{"base":{"ref":"main"},"head":{"ref":"f"}}}"#,
    )
    .unwrap();
    let out =
        run(&args(&[&w, "--runner-dir", &r, "--dry-run", "--no-cache", "--event", "pull_request", "--event-path", &p]));
    assert_eq!(out.code, 78, "{}{}", out.stdout, out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// #242: `--offline` reads the store of actions only: a store that the user
/// cannot write is no refusal. #244: a runner directory that cannot take
/// the lock fails a dry run (as a user with no root only: root writes each
/// directory).
#[test]
fn the_paths_of_a_run_by_their_use() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("pathuse");
    std::fs::write(dir.join("w.yml"), WORKFLOW).unwrap();
    let w = dir.join("w.yml").display().to_string();
    let store = dir.join("store");
    std::fs::create_dir_all(&store).unwrap();
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o555)).unwrap();
    let s = store.display().to_string();
    let r = runner_dir("pathuse");
    let out = run(&args(&[&w, "--runner-dir", &r, "--dry-run", "--no-cache", "--offline", "--actions", &s]));
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    // SAFETY: geteuid has no failure.
    if unsafe { libc::geteuid() } != 0 {
        std::fs::set_permissions(&r, std::fs::Permissions::from_mode(0o555)).unwrap();
        let out = run(&args(&[&w, "--runner-dir", &r, "--dry-run", "--no-cache"]));
        std::fs::set_permissions(&r, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(out.code, 78, "{}{}", out.stdout, out.stderr);
        assert!(out.stderr.contains(".podrunner-run.lock"), "{}", out.stderr);
    }
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o755)).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// #238: the actions of a called workflow of the tree are pinned before the
/// first job: with `--offline` and no pin, a dry run fails on them.
#[test]
fn the_actions_of_a_called_workflow_are_pinned() {
    let dir = scratch("called");
    let tree = dir.join("tree");
    std::fs::create_dir_all(tree.join(".github/workflows")).unwrap();
    std::fs::write(
        tree.join(".github/workflows/reuse.yml"),
        "on: workflow_call\njobs:\n  inner:\n    runs-on: ubuntu-latest\n    steps: [{uses: owner/action@v1}]\n",
    )
    .unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  call:\n    uses: ./.github/workflows/reuse.yml\n").unwrap();
    let (w, t, store) =
        (dir.join("w.yml").display().to_string(), tree.display().to_string(), dir.join("store").display().to_string());
    std::fs::create_dir_all(dir.join("store")).unwrap();
    let r = runner_dir("called");
    let out = run(&args(&[
        &w,
        "--runner-dir",
        &r,
        "--dry-run",
        "--no-cache",
        "--offline",
        "--checkout",
        &t,
        "--actions",
        &store,
    ]));
    assert_eq!(out.code, 78, "{}{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("owner/action@v1"), "{}", out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-139: `--local-repository` serves a directory as an action; the lock
/// stays as it was, and the forge is not asked (`--offline`).
#[test]
fn a_local_repository_serves_an_action() {
    let dir = std::env::temp_dir().join(format!("podrunner-localrepo-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("tool")).unwrap();
    std::fs::write(dir.join("tool/action.yml"), "runs:\n  using: composite\n  steps: [{run: hi, shell: sh}]\n")
        .unwrap();
    let file = dir.join("w.yml");
    std::fs::write(&file, "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{uses: me/tool@main}]\n").unwrap();
    let (w, local) = (file.display().to_string(), format!("me/tool@main={}", dir.join("tool").display()));
    let store = dir.join("store").display().to_string();
    let base = [w.as_str(), "--dry-run", "--no-cache", "--offline", "--actions", &store];
    let out = run(&args(&[&base[..], &["--local-repository", &local]].concat()));
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("planned a\n"), "{}", out.stdout);
    assert!(!dir.join("podrunner-actions.lock").exists(), "a local reference is not in the lock");
    let out = run(&args(&base));
    assert_ne!(out.code, 0, "with no --local-repository, --offline has no pin: {}", out.stdout);
    assert!(run(&args(&[&w, "--local-repository", "me/tool"])).stderr.contains("OWNER/REPO@REF=DIR"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// #261: a checkout of another repository, or a called workflow, with a
/// part `..` is refused before any fetch.
#[test]
fn a_repository_with_dot_dot_is_refused_before_a_fetch() {
    let dir = scratch("dotdot");
    let flows = [
        "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n        with:\n          repository: ../x\n",
        "on: push\njobs:\n  a:\n    uses: ../x/.github/workflows/w.yml@v1\n",
        "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{uses: ../x@v1}]\n",
    ];
    for (i, flow) in flows.iter().enumerate() {
        let file = dir.join(format!("w{i}.yml"));
        std::fs::write(&file, flow).unwrap();
        let store = dir.join("store").display().to_string();
        let w = file.display().to_string();
        let out =
            run(&args(&[&w, "--dry-run", "--no-cache", "--actions", &store, "--runner-dir", &runner_dir("dotdot")]));
        assert_ne!(out.code, 0, "{i}: {}{}", out.stdout, out.stderr);
        assert!(out.stderr.contains("not a name"), "{i}: {}", out.stderr);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
