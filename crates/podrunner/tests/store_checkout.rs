//! A checkout in a composite action of the store (T-718). The actions come
//! from `--local-repository`, so the run asks no forge. Ignored by default:
//! it needs an unpacked release of `actions/runner`.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner --test store_checkout -- --ignored

mod scratch;

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

/// T-718: a checkout in a composite action of the store, also a nested
/// one, with a static `ref` (a tag of the local git) or `repository`
/// (`--local-repository`) gets the tree that the run pinned for it; with an
/// expression, its step fails with the fix. The run asks no forge, also
/// not for `actions/checkout`.
#[test]
#[ignore]
fn a_checkout_in_an_action_of_the_store_gets_its_pinned_tree() {
    let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR");
    let scratch = scratch::Scratch::new("store-action-trees");
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
    std::fs::write(
        repo.join("co.yml"),
        "on: push\njobs:\n  co:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n      \
         - uses: o/act@v1\n      - run: test \"$(cat a.txt)\" = head\n  \
         bad:\n    runs-on: x\n    steps:\n      - uses: o/act/any@v1\n        with: {repo: o/second}\n",
    )
    .unwrap();
    git(&["add", "-A"]);
    git(&commit);
    let put = |name: &str, file: &str, text: &str| {
        let d = scratch.join(name);
        std::fs::create_dir_all(d.join(file).parent().unwrap()).unwrap();
        std::fs::write(d.join(file), text).unwrap();
        d.display().to_string()
    };
    let act = put("act", "action.yml", ACT);
    put("act", "any/action.yml", ANY);
    let local = [
        format!("o/act@v1={act}"),
        format!("o/nested@v2={}", put("nested", "action.yml", NESTED)),
        format!("o/second@v1={}", put("second", "b.txt", "second-v1\n")),
        format!("o/third@v3={}", put("third", "c.txt", "third-v3\n")),
    ];
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
        args.extend(["--local-repository".to_string(), l]);
    }
    let out = podrunner::cli::run(&args);
    println!("{}{}", out.stdout, out.stderr);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.lines().any(|l| l == "ok      co"), "{}", out.stdout);
    assert!(out.stdout.lines().any(|l| l.starts_with("failure bad")), "{}", out.stdout);
    let mut text = String::new();
    let mut dirs = vec![logs.clone()];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            match e.file_type().unwrap().is_dir() {
                true => dirs.push(e.path()),
                false => text.push_str(&std::fs::read_to_string(e.path()).unwrap_or_default()),
            }
        }
    }
    let want = "the checkout of o/second in an action: the run pinned no tree for it; fix:";
    assert!(text.contains(want), "{want:?} in the logs:\n{text}");
    // No lock pins a reference of `--local-repository`, nor the checkout.
    let lock = std::fs::read_to_string(repo.join("podrunner-actions.lock")).unwrap_or_default();
    assert!(!lock.contains("actions/checkout"), "{lock}");
}
