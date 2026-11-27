//! A checkout in a composite action of the store (T-718), and a local
//! action that one names (T-725). The actions come from
//! `--local-repository`, so the run asks no forge. Ignored by default: they
//! need an unpacked release of `actions/runner`.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner --test store_checkout -- --ignored

mod scratch;

use std::path::{Path, PathBuf};

/// The tests of this file share one runner directory: one runs at a time.
static RUNNER: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A composite action of the store that checks out the tree of a tag of
/// the repository of the run and a second repository at a tag, and names
/// a nested composite action of the store.
const ACT: &str = "runs:\n  using: composite\n  steps:\n    - uses: actions/checkout@v4\n      \
                   with: {ref: v1, path: same}\n    - uses: actions/checkout@v4\n      \
                   with: {repository: o/second, ref: v1, path: second}\n    - uses: o/nested@v2\n    \
                   - shell: bash\n      run: test \"$(cat same/a.txt)\" = tag-v1 && test \"$(cat second/b.txt)\" = \
                   second-v1 && test \"$(cat third/c.txt)\" = third-v3 && test ! -e second/a.txt\n";

/// The nested composite action: a checkout of a third repository.
const NESTED: &str = "runs:\n  using: composite\n  steps:\n    - uses: actions/checkout@v4\n      \
                      with: {repository: o/third, ref: v3, path: third}\n";

/// A composite action of the store whose checkout names its repository
/// with an expression: the run cannot pin its tree.
const ANY: &str = "inputs:\n  repo:\n    required: true\nruns:\n  using: composite\n  steps:\n    \
                   - uses: actions/checkout@v4\n      with: {repository: '${{ inputs.repo }}', path: x}\n";

/// A git repository `repo` in `scratch`: `a.txt` is `tag-v1` at the tag
/// `v1` and `head` at HEAD; `files` are in the commit of HEAD.
fn repo_with(scratch: &Path, files: &[(&str, &str)]) -> PathBuf {
    let repo = scratch.join("repo");
    let git = |a: &[&str]| {
        let o = std::process::Command::new("git").arg("-C").arg(&repo).args(a).output().unwrap();
        assert!(o.status.success(), "{a:?}: {}", String::from_utf8_lossy(&o.stderr));
    };
    let commit = ["-c", "user.name=T", "-c", "user.email=t@example.invalid", "commit", "-q", "-m", "c"];
    std::fs::create_dir_all(&repo).unwrap();
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "tag-v1\n").unwrap();
    git(&["add", "a.txt"]);
    git(&commit);
    git(&["tag", "v1"]);
    std::fs::write(repo.join("a.txt"), "head\n").unwrap();
    for (name, text) in files {
        std::fs::create_dir_all(repo.join(name).parent().unwrap()).unwrap();
        std::fs::write(repo.join(name), text).unwrap();
    }
    git(&["add", "-A"]);
    git(&commit);
    repo
}

/// The file `file` with `text` in the directory `name` of `scratch`; the
/// directory.
fn put(scratch: &Path, name: &str, file: &str, text: &str) -> String {
    let d = scratch.join(name);
    std::fs::create_dir_all(d.join(file).parent().unwrap()).unwrap();
    std::fs::write(d.join(file), text).unwrap();
    d.display().to_string()
}

/// `podrunner run` of `co.yml` of `repo`, offline, with each `local` as a
/// `--local-repository`: the output, and the text of all its logs.
fn run(scratch: &Path, repo: &Path, local: &[String]) -> (podrunner::cli::Out, String) {
    let _one = RUNNER.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR");
    let logs = scratch.join("logs");
    let mut args: Vec<String> = [
        "run",
        repo.join("co.yml").to_str().unwrap(),
        "--runner-dir",
        &dir,
        "--checkout",
        repo.to_str().unwrap(),
        "--actions",
        scratch.join("store").to_str().unwrap(),
        "--log-dir",
        logs.to_str().unwrap(),
        "--no-cache",
        "--offline",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for l in local {
        args.extend(["--local-repository".to_string(), l.clone()]);
    }
    let out = podrunner::cli::run(&args);
    println!("{}{}", out.stdout, out.stderr);
    let mut text = String::new();
    let mut dirs = vec![logs];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            match e.file_type().unwrap().is_dir() {
                true => dirs.push(e.path()),
                false => text.push_str(&std::fs::read_to_string(e.path()).unwrap_or_default()),
            }
        }
    }
    (out, text)
}

/// T-718: a checkout in a composite action of the store, also a nested
/// one, with a static `ref` (a tag of the local git) or `repository`
/// (`--local-repository`) gets the tree that the run pinned for it; with an
/// expression, its step fails with the fix. The run asks no forge, also
/// not for `actions/checkout`.
#[test]
#[ignore]
fn a_checkout_in_an_action_of_the_store_gets_its_pinned_tree() {
    let scratch = scratch::Scratch::new("store-action-trees");
    let co = "on: push\njobs:\n  co:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n      \
              - uses: o/act@v1\n      - run: test \"$(cat a.txt)\" = head\n  \
              bad:\n    runs-on: x\n    steps:\n      - uses: o/act/any@v1\n        with: {repo: o/second}\n";
    let repo = repo_with(&scratch, &[("co.yml", co)]);
    let act = put(&scratch, "act", "action.yml", ACT);
    put(&scratch, "act", "any/action.yml", ANY);
    let local = [
        format!("o/act@v1={act}"),
        format!("o/nested@v2={}", put(&scratch, "nested", "action.yml", NESTED)),
        format!("o/second@v1={}", put(&scratch, "second", "b.txt", "second-v1\n")),
        format!("o/third@v3={}", put(&scratch, "third", "c.txt", "third-v3\n")),
    ];
    let (out, text) = run(&scratch, &repo, &local);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.lines().any(|l| l == "ok      co"), "{}", out.stdout);
    assert!(out.stdout.lines().any(|l| l.starts_with("failure bad")), "{}", out.stdout);
    let want = "the checkout of o/second in an action: the run pinned no tree for it; fix:";
    assert!(text.contains(want), "{want:?} in the logs:\n{text}");
    // No lock pins a reference of `--local-repository`, nor the checkout.
    let lock = std::fs::read_to_string(repo.join("podrunner-actions.lock")).unwrap_or_default();
    assert!(!lock.contains("actions/checkout"), "{lock}");
}

/// T-725: a composite action of the store names `./local`, a local action
/// of the workspace. Its remote `uses:` (`o/rem@v1`), the tree of its
/// checkout of a second repository and the tree of its checkout of a tag
/// of the repository of the run are pinned, as for a local action of the
/// workflow.
#[test]
#[ignore]
fn a_local_action_that_an_action_of_the_store_names_is_pinned() {
    let scratch = scratch::Scratch::new("store-local-action");
    let co = "on: push\njobs:\n  co:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n      \
              - uses: o/loc@v1\n      - run: test \"$(cat rem.txt)\" = rem && test \"$(cat a.txt)\" = head\n";
    let local_action = "runs:\n  using: composite\n  steps:\n    - uses: o/rem@v1\n    \
                        - uses: actions/checkout@v4\n      with: {repository: o/second, ref: v1, path: second}\n    \
                        - uses: actions/checkout@v4\n      with: {ref: v1, path: same}\n    \
                        - shell: bash\n      run: test \"$(cat second/b.txt)\" = second-v1 && \
                        test \"$(cat same/a.txt)\" = tag-v1\n";
    let repo = repo_with(&scratch, &[("co.yml", co), ("local/action.yml", local_action)]);
    let loc = "runs:\n  using: composite\n  steps:\n    - uses: ./local\n";
    let rem = "runs:\n  using: composite\n  steps:\n    - shell: bash\n      \
               run: echo rem > \"$GITHUB_WORKSPACE/rem.txt\"\n";
    let local = [
        format!("o/loc@v1={}", put(&scratch, "loc", "action.yml", loc)),
        format!("o/rem@v1={}", put(&scratch, "rem", "action.yml", rem)),
        format!("o/second@v1={}", put(&scratch, "second", "b.txt", "second-v1\n")),
    ];
    let (out, text) = run(&scratch, &repo, &local);
    assert_eq!(out.code, 0, "{}{}\nthe logs:\n{text}", out.stdout, out.stderr);
    assert!(out.stdout.lines().any(|l| l == "ok      co"), "{}", out.stdout);
}

/// T-726: GitHub matches the owner and the name of an action with no case.
/// `actions/checkout` by any case gets the stand-in in a step, in a local
/// action and in an action of the store; the runner asks the broker with
/// the name as the job wrote it, and the lock pins no checkout.
#[test]
#[ignore]
fn a_checkout_by_any_case_gets_the_stand_in() {
    let scratch = scratch::Scratch::new("store-checkout-case");
    let co = "on: push\njobs:\n  co:\n    runs-on: x\n    steps:\n      - uses: Actions/Checkout@v4\n      \
              - uses: ./setup\n      - uses: o/act@v1\n      - run: test \"$(cat a.txt)\" = head && \
              test \"$(cat second/b.txt)\" = second-v1 && test \"$(cat same/a.txt)\" = tag-v1\n";
    let setup = "runs:\n  using: composite\n  steps:\n    - uses: ACTIONS/checkout@v4\n      \
                 with: {repository: o/second, ref: v1, path: second}\n";
    let repo = repo_with(&scratch, &[("co.yml", co), ("setup/action.yml", setup)]);
    let act = "runs:\n  using: composite\n  steps:\n    - uses: actions/CHECKOUT@v4\n      \
               with: {ref: v1, path: same}\n";
    let local = [
        format!("o/act@v1={}", put(&scratch, "act", "action.yml", act)),
        format!("o/second@v1={}", put(&scratch, "second", "b.txt", "second-v1\n")),
    ];
    let (out, text) = run(&scratch, &repo, &local);
    assert_eq!(out.code, 0, "{}{}\nthe logs:\n{text}", out.stdout, out.stderr);
    assert!(out.stdout.lines().any(|l| l == "ok      co"), "{}", out.stdout);
    let lock = std::fs::read_to_string(repo.join("podrunner-actions.lock")).unwrap_or_default();
    assert!(!lock.to_ascii_lowercase().contains("actions/checkout"), "{lock}");
}

/// T-729: `uses: $/PATH` names an action of the repository of the workflow
/// at its commit: the tree of the run. A step and a composite action of
/// that tree with `$/PATH` run it with no checkout, and the remote `uses:`
/// of such an action is pinned. In a composite action of the store, the
/// run refuses `$/PATH` with the fix.
#[test]
#[ignore]
fn a_uses_of_the_repository_of_the_workflow_runs_its_tree() {
    let scratch = scratch::Scratch::new("store-self-uses");
    let co = "on: push\njobs:\n  s:\n    runs-on: x\n    steps:\n      - id: a\n        uses: $/acts/a\n      \
              - uses: $/acts/b\n      - run: test \"${{ steps.a.outputs.x }}\" = a-b && test \"$(cat b.txt)\" = b \
              && test \"$(cat rem.txt)\" = rem && test ! -e a.txt\n";
    let a = "outputs:\n  x:\n    value: ${{ steps.n.outputs.x }}\nruns:\n  using: composite\n  steps:\n    \
             - uses: $/acts/b\n    - uses: o/rem@v1\n    - id: n\n      shell: bash\n      \
             run: echo \"x=a-$(cat \"$GITHUB_WORKSPACE/b.txt\")\" >> \"$GITHUB_OUTPUT\"\n";
    let b = "runs:\n  using: composite\n  steps:\n    - shell: bash\n      \
             run: echo b > \"$GITHUB_WORKSPACE/b.txt\"\n";
    let repo = repo_with(&scratch, &[("co.yml", co), ("acts/a/action.yml", a), ("acts/b/action.yml", b)]);
    let rem = "runs:\n  using: composite\n  steps:\n    - shell: bash\n      \
               run: echo rem > \"$GITHUB_WORKSPACE/rem.txt\"\n";
    let mut local = vec![format!("o/rem@v1={}", put(&scratch, "rem", "action.yml", rem))];
    let (out, text) = run(&scratch, &repo, &local);
    assert_eq!(out.code, 0, "{}{}\nthe logs:\n{text}", out.stdout, out.stderr);
    assert!(out.stdout.lines().any(|l| l == "ok      s"), "{}", out.stdout);
    // An action of the store with `$/PATH`: the run stops before a job.
    let sact = "runs:\n  using: composite\n  steps:\n    - uses: $/x\n";
    local.push(format!("o/sact@v1={}", put(&scratch, "sact", "action.yml", sact)));
    std::fs::write(repo.join("co.yml"), "on: push\njobs:\n  s:\n    runs-on: x\n    steps: [{uses: o/sact@v1}]\n")
        .unwrap();
    let (out, _) = run(&scratch, &repo, &local);
    assert!(out.code != 0 && !out.stdout.contains("ok "), "{}{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("`uses: $/x`") && out.stderr.contains("fix:"), "{}", out.stderr);
}
