//! `podrunner run` runs the suite of the local mode to green on the official
//! runner (T-089), with the tree of this repository for `actions/checkout`
//! (T-092). Ignored by default: it needs an unpacked release of
//! `actions/runner` in a directory it may change.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner --test local -- --ignored

/// The tests of this file share one runner directory: one runs at a time.
static RUNNER: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
#[ignore]
fn podrunner_run_runs_the_local_suite_to_green() {
    let _one = RUNNER.lock().unwrap_or_else(|e| e.into_inner());
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
    let _one = RUNNER.lock().unwrap_or_else(|e| e.into_inner());
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

/// T-140: a job with `id-token: write` gets a signed token from the broker,
/// with the audience that it asks for; a job with no permission gets no URL.
/// A called job takes the permissions of its caller (T-416). The runner
/// gives `GITHUB_WORKFLOW` and `GITHUB_WORKFLOW_REF` (T-426).
#[test]
#[ignore]
fn a_job_with_id_token_write_gets_a_signed_token() {
    let _one = RUNNER.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR");
    let scratch = std::env::temp_dir().join(format!("podrunner-local-oidc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let flow = scratch.join("oidc.yml");
    // The payload is base64url with no padding: python3 decodes it.
    std::fs::write(
        &flow,
        "on: push\njobs:\n  token:\n    runs-on: x\n    permissions:\n      id-token: write\n    steps:\n      \
         - run: |\n          curl -fsS -H \"Authorization: bearer $ACTIONS_ID_TOKEN_REQUEST_TOKEN\" \
         \"$ACTIONS_ID_TOKEN_REQUEST_URL&audience=sts.example\" > t.json\n          \
         python3 -c 'import json,base64; t=json.load(open(\"t.json\"))[\"value\"].split(\".\")[1]; \
         c=json.loads(base64.urlsafe_b64decode(t+\"==\")); assert c[\"aud\"]==\"sts.example\", c; \
         assert c[\"iss\"]==\"https://oidc.example.invalid\", c'\n  \
         none:\n    runs-on: x\n    steps:\n      - run: test -z \"$ACTIONS_ID_TOKEN_REQUEST_URL\"\n      \
         - run: test \"$GITHUB_WORKFLOW\" = oidc.yml && case \"$GITHUB_WORKFLOW_REF\" in */oidc.yml@refs/*) ;; *) exit 1;; esac\n  \
         called:\n    permissions:\n      id-token: write\n    uses: ./.github/workflows/called.yml\n",
    )
    .unwrap();
    std::fs::create_dir_all(scratch.join(".github/workflows")).unwrap();
    std::fs::write(
        scratch.join(".github/workflows/called.yml"),
        "on: workflow_call\njobs:\n  t:\n    runs-on: x\n    steps:\n      - run: test -n \"$ACTIONS_ID_TOKEN_REQUEST_URL\"\n",
    )
    .unwrap();
    let keys = scratch.join("jwks.json");
    let args: Vec<String> = [
        "run",
        flow.to_str().unwrap(),
        "--runner-dir",
        &dir,
        "--oidc-issuer",
        "https://oidc.example.invalid",
        "--oidc-keys",
        keys.to_str().unwrap(),
        "--checkout",
        scratch.to_str().unwrap(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let out = podrunner::cli::run(&args);
    println!("{}{}", out.stdout, out.stderr);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(std::fs::read_to_string(&keys).unwrap().contains("\"P-256\""));
    assert!(out.stdout.lines().any(|l| l.starts_with("ok") && l.contains("called / t")), "{}", out.stdout);
    let _ = std::fs::remove_dir_all(&scratch);
}

/// T-419, T-420: `--protected` holds a job of a called workflow and a job
/// whose environment is an expression; `--approve` runs them.
#[test]
#[ignore]
fn a_protected_environment_holds_a_called_job_and_an_expression() {
    let _one = RUNNER.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR");
    let scratch = std::env::temp_dir().join(format!("podrunner-local-protect-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(scratch.join(".github/workflows")).unwrap();
    let flow = scratch.join("protect.yml");
    std::fs::write(
        &flow,
        "on: push\njobs:\n  free:\n    runs-on: x\n    steps: [{run: 'true'}]\n  pick:\n    runs-on: x\n    \
         environment: ${{ vars.TARGET }}\n    steps: [{run: 'true'}]\n  call:\n    uses: ./.github/workflows/rel.yml\n",
    )
    .unwrap();
    std::fs::write(
        scratch.join(".github/workflows/rel.yml"),
        "on: workflow_call\njobs:\n  r:\n    runs-on: x\n    environment: prod\n    steps: [{run: 'true'}]\n",
    )
    .unwrap();
    let run = |more: &[&str]| {
        let mut args: Vec<String> =
            ["run", flow.to_str().unwrap(), "--runner-dir", &dir, "--checkout"].iter().map(|s| s.to_string()).collect();
        args.push(scratch.display().to_string());
        args.extend(["--var", "TARGET=prod", "--protected", "prod", "-q"].iter().map(|s| s.to_string()));
        args.extend(more.iter().map(|s| s.to_string()));
        let out = podrunner::cli::run(&args);
        println!("{}{}", out.stdout, out.stderr);
        out
    };
    let out = run(&[]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    let line =
        |job: &str| out.stdout.lines().find(|l| l.ends_with(job) || l.contains(&format!(" {job} "))).unwrap_or("");
    assert!(line("free").starts_with("ok"), "{}", out.stdout);
    for job in ["pick", "call / r"] {
        let l = out.stdout.lines().find(|l| l.contains(job)).unwrap_or("");
        assert!(l.starts_with("failure") && l.contains("--approve prod"), "{job}: {}", out.stdout);
    }
    let out = run(&["--approve", "prod"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let _ = std::fs::remove_dir_all(&scratch);
}

/// T-425: the checkout of the local tree keeps the submodules of its
/// `submodules:`, and leaves each other one an empty directory.
#[test]
#[ignore]
fn a_checkout_keeps_the_submodules_that_it_asks_for() {
    let _one = RUNNER.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR");
    let base = std::env::temp_dir().join(format!("podrunner-local-submodules-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let git = |d: &std::path::Path, args: &[&str]| {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(d)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "protocol.file.allow=always"])
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    };
    let init = |name: &str| {
        let d = base.join(name);
        std::fs::create_dir_all(&d).unwrap();
        git(&d, &["init", "-q", "-b", "main"]);
        std::fs::write(d.join(format!("{name}.txt")), name).unwrap();
        d
    };
    let commit = |d: &std::path::Path| {
        git(d, &["add", "-A"]);
        git(d, &["commit", "-qm", "c"]);
    };
    let inner = init("inner");
    commit(&inner);
    let lib = init("lib");
    git(&lib, &["submodule", "add", "-q", inner.to_str().unwrap(), "nested"]);
    commit(&lib);
    let top = init("top");
    git(&top, &["submodule", "add", "-q", lib.to_str().unwrap(), "lib"]);
    git(&top, &["submodule", "update", "-q", "--init", "--recursive"]);
    let flow = base.join("subs.yml");
    std::fs::write(
        &flow,
        "on: push\njobs:\n  s:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n        with: {path: none}\n      \
         - uses: actions/checkout@v4\n        with: {path: one, submodules: true}\n      \
         - uses: actions/checkout@v4\n        with: {path: all, submodules: recursive}\n      \
         - run: |\n          test -d none/lib && test -z \"$(ls -A none/lib)\"\n          \
         test -f one/lib/lib.txt && test -d one/lib/nested && test -z \"$(ls -A one/lib/nested)\"\n          \
         test -f all/lib/nested/inner.txt && test ! -e all/.podrunner-submodules.list\n",
    )
    .unwrap();
    commit(&top);
    let run = |checkout: &std::path::Path| {
        let args: Vec<String> =
            ["run", flow.to_str().unwrap(), "--runner-dir", &dir, "-q", "--checkout", checkout.to_str().unwrap()]
                .iter()
                .map(|s| s.to_string())
                .collect();
        let out = podrunner::cli::run(&args);
        println!("{}{}", out.stdout, out.stderr);
        out
    };
    let out = run(&top);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    // A clone with no `submodule update`: a checkout that asks for the
    // submodule fails with the fix.
    let clone = base.join("clone");
    git(&base, &["clone", "-q", top.to_str().unwrap(), clone.to_str().unwrap()]);
    let out = run(&clone);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("git submodule update --init --recursive"), "{}", out.stderr);
    let _ = std::fs::remove_dir_all(&base);
}

/// T-428: a directory run with `--state` keeps the state of each workflow
/// in a directory of its own.
#[test]
#[ignore]
fn a_directory_run_keeps_the_state_of_each_workflow() {
    let _one = RUNNER.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR");
    let scratch = std::env::temp_dir().join(format!("podrunner-local-many-state-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let flows = scratch.join("flows");
    std::fs::create_dir_all(&flows).unwrap();
    for name in ["a", "b"] {
        let text = format!("on: push\njobs:\n  {name}:\n    runs-on: x\n    steps: [{{run: echo {name}}}]\n");
        std::fs::write(flows.join(format!("{name}.yml")), text).unwrap();
    }
    let state = scratch.join("state");
    let args: Vec<String> =
        ["run", flows.to_str().unwrap(), "--runner-dir", &dir, "-q", "--state", state.to_str().unwrap()]
            .iter()
            .map(|s| s.to_string())
            .collect();
    let out = podrunner::cli::run(&args);
    println!("{}{}", out.stdout, out.stderr);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    for (name, job) in [("a.yml", "\"a.0\""), ("b.yml", "\"b.0\"")] {
        let s = std::fs::read_to_string(state.join(name).join("state.json")).unwrap();
        assert!(s.contains(job), "{name}: {s}");
    }
    let _ = std::fs::remove_dir_all(&scratch);
}

/// T-427: a rerun of a failed job keeps a call that passed; it does not
/// run the jobs of the call again.
#[test]
#[ignore]
fn a_rerun_keeps_a_call_that_passed() {
    let _one = RUNNER.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR");
    let scratch = std::env::temp_dir().join(format!("podrunner-local-rerun-call-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(scratch.join(".github/workflows")).unwrap();
    let marker = scratch.join("ok");
    let flow = scratch.join("w.yml");
    let text = format!(
        "on: push\njobs:\n  call:\n    uses: ./.github/workflows/c.yml\n  bad:\n    runs-on: x\n    steps: [{{run: test -f '{}'}}]\n",
        marker.display()
    );
    std::fs::write(&flow, text).unwrap();
    std::fs::write(
        scratch.join(".github/workflows/c.yml"),
        "on: workflow_call\njobs:\n  b:\n    runs-on: x\n    steps: [{run: 'true'}]\n",
    )
    .unwrap();
    let state = scratch.join("state").display().to_string();
    let base = ["run", flow.to_str().unwrap(), "--runner-dir", &dir, "-q", "--checkout", scratch.to_str().unwrap()];
    let run = |more: &[&str]| {
        let args: Vec<String> = base.iter().chain(more).map(|s| s.to_string()).collect();
        let out = podrunner::cli::run(&args);
        println!("{}{}", out.stdout, out.stderr);
        out
    };
    let out = run(&["--state", &state]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    std::fs::write(&marker, "").unwrap();
    let out = run(&["--rerun", &state]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.lines().any(|l| l.starts_with("kept") && l.contains("call")), "{}", out.stdout);
    assert!(out.stdout.lines().any(|l| l.starts_with("ok") && l.ends_with("bad")), "{}", out.stdout);
    let _ = std::fs::remove_dir_all(&scratch);
}

/// T-640: a checkout of the same repository with `ref:` gives the commit of
/// the ref in the local git (with its `.git` at that commit), not the
/// working tree; a ref with no commit fails its step with the name.
#[test]
#[ignore]
fn a_checkout_with_a_ref_gives_the_commit_of_the_ref() {
    let _one = RUNNER.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR");
    let scratch = std::env::temp_dir().join(format!("podrunner-local-ref-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let repo = scratch.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |a: &[&str]| {
        let o = std::process::Command::new("git").arg("-C").arg(&repo).args(a).output().unwrap();
        assert!(o.status.success(), "{a:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8(o.stdout).unwrap().trim().to_string()
    };
    let commit = ["-c", "user.name=T", "-c", "user.email=t@example.invalid", "commit", "-q", "-m", "c"];
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "main\n").unwrap();
    git(&["add", "a.txt"]);
    git(&commit);
    let main = git(&["rev-parse", "HEAD"]);
    git(&["checkout", "-q", "-b", "feature"]);
    std::fs::write(repo.join("a.txt"), "feature\n").unwrap();
    git(&["add", "a.txt"]);
    git(&commit);
    std::fs::write(
        repo.join("co.yml"),
        format!(
            "on: push\njobs:\n  co:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n        \
             with: {{ref: main, path: other}}\n      - run: test \"$(cat other/a.txt)\" = main && test \"$(git -C other \
             rev-parse HEAD)\" = {main}\n      - uses: actions/checkout@v4\n      - run: test \"$(cat a.txt)\" = feature\n  \
             bad:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n        with:\n          ref: nosuch\n"
        ),
    )
    .unwrap();
    let args: Vec<String> = ["run", repo.join("co.yml").to_str().unwrap(), "--runner-dir", &dir, "--checkout"]
        .iter()
        .map(|s| s.to_string())
        .chain([repo.display().to_string()])
        .collect();
    let out = podrunner::cli::run(&args);
    println!("{}{}", out.stdout, out.stderr);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.lines().any(|l| l == "ok      co"), "{}", out.stdout);
    assert!(out.stdout.lines().any(|l| l.starts_with("failure bad")), "{}", out.stdout);
    let _ = std::fs::remove_dir_all(&scratch);
}
