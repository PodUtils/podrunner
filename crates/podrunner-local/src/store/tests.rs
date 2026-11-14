use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

/// A forge in memory: each `OWNER/REPO@REF` names a commit, and each commit
/// has files. It counts its calls.
#[derive(Default)]
struct Fake {
    refs: RefCell<BTreeMap<String, String>>,
    files: RefCell<BTreeMap<String, Vec<(String, String)>>>,
    resolves: Cell<u32>,
    tars: Cell<u32>,
}

impl Fake {
    fn add(&self, key: &str, commit: &str, files: &[(&str, &str)]) {
        self.refs.borrow_mut().insert(key.into(), commit.into());
        let files = files.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect();
        self.files.borrow_mut().insert(commit.into(), files);
    }
}

impl Forge for Fake {
    fn resolve(&self, repo: &str, r: &str) -> Result<String, String> {
        self.resolves.set(self.resolves.get() + 1);
        self.refs.borrow().get(&format!("{repo}@{r}")).cloned().ok_or_else(|| format!("no {repo}@{r}"))
    }

    fn tar(&self, _repo: &str, commit: &str) -> Result<Vec<u8>, String> {
        self.tars.set(self.tars.get() + 1);
        let files = self.files.borrow().get(commit).cloned().ok_or("no commit")?;
        Ok(tar_of(&files))
    }
}

fn tar_of(files: &[(String, String)]) -> Vec<u8> {
    let mut b = tar::Builder::new(Vec::new());
    for (name, text) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(text.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        b.append_data(&mut h, format!("top/{name}"), text.as_bytes()).unwrap();
    }
    b.into_inner().unwrap()
}

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const C: &str = "cccccccccccccccccccccccccccccccccccccccc";

const NODE: &str = "runs:\n  using: node20\n  main: index.js\n";

/// A forge with a composite action by tag that names a second action by
/// tag, and an action in a path of a repository.
fn forge() -> Fake {
    let f = Fake::default();
    let composite = "runs:\n  using: composite\n  steps:\n    - uses: o/inner@v2\n    - uses: ./local\n    - run: true\n      shell: sh\n";
    f.add("o/outer@v1", A, &[("action.yml", composite)]);
    f.add("o/inner@v2", B, &[("action.yml", NODE)]);
    f.add("o/multi@main", C, &[("sub/dir/action.yaml", NODE)]);
    f
}

fn store() -> Store {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "podrunner-store-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    Store { dir }
}

fn uses(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_composite_action_by_tag_brings_the_action_that_it_names() {
    let (f, s) = (forge(), store());
    let r =
        resolve(&uses(&["o/outer@v1", "o/multi/sub/dir@main"]), &Lock::default(), &s, &f, Mode::Online, "L").unwrap();
    assert_eq!(r.commits["o/outer@v1"], A);
    assert_eq!(r.commits["o/inner@v2"], B, "the nested action is resolved");
    assert_eq!(r.commits["o/multi@main"], C);
    assert!(r.changed);
    assert_eq!(r.lock.pins.len(), 3);
    for (repo, c) in [("o/outer", A), ("o/inner", B), ("o/multi", C)] {
        assert!(s.path(repo, c).is_file(), "{repo} is in the store");
    }
    // A second run takes the lock and the store: no tar, one check of each tag.
    let (resolves, tars) = (f.resolves.get(), f.tars.get());
    let again = resolve(&uses(&["o/outer@v1"]), &r.lock, &s, &f, Mode::Online, "L").unwrap();
    assert!(!again.changed);
    assert_eq!((f.tars.get(), f.resolves.get()), (tars, resolves + 2));
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn a_moved_tag_fails_by_name() {
    let (f, s) = (forge(), store());
    let r = resolve(&uses(&["o/inner@v2"]), &Lock::default(), &s, &f, Mode::Online, "L").unwrap();
    f.add("o/inner@v2", C, &[("action.yml", NODE)]);
    let e = resolve(&uses(&["o/inner@v2"]), &r.lock, &s, &f, Mode::Online, "L").unwrap_err();
    assert!(e.contains("`o/inner@v2` moved") && e.contains(B) && e.contains(C), "{e}");
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn offline_takes_the_lock_and_the_store_and_names_what_is_missing() {
    let (f, s) = (forge(), store());
    let r = resolve(&uses(&["o/outer@v1"]), &Lock::default(), &s, &f, Mode::Online, "L").unwrap();
    let (resolves, tars) = (f.resolves.get(), f.tars.get());
    let off = resolve(&uses(&["o/outer@v1"]), &r.lock, &s, &f, Mode::Offline, "L").unwrap();
    assert_eq!(off.commits, r.commits);
    assert_eq!((f.resolves.get(), f.tars.get()), (resolves, tars), "offline asks the forge nothing");
    let e = resolve(&uses(&["o/multi/sub/dir@main"]), &r.lock, &s, &f, Mode::Offline, "L").unwrap_err();
    assert!(e.contains("`o/multi@main` is not in L"), "{e}");
    std::fs::remove_file(s.path("o/inner", B)).unwrap();
    let e = resolve(&uses(&["o/outer@v1"]), &r.lock, &s, &f, Mode::Offline, "L").unwrap_err();
    assert!(e.contains("not in the store"), "{e}");
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn frozen_refuses_a_change_of_the_lock_and_fetches_what_the_lock_pins() {
    let (f, s) = (forge(), store());
    let e = resolve(&uses(&["o/inner@v2"]), &Lock::default(), &s, &f, Mode::Frozen, "L").unwrap_err();
    assert!(e.contains("--frozen"), "{e}");
    let e = resolve(&uses(&[&format!("o/inner@{B}")]), &Lock::default(), &s, &f, Mode::Frozen, "L").unwrap_err();
    assert!(e.contains("--frozen"), "a commit not in the lock: {e}");
    let other = store();
    let lock = resolve(&uses(&["o/inner@v2"]), &Lock::default(), &other, &f, Mode::Online, "L").unwrap().lock;
    let _ = std::fs::remove_dir_all(&other.dir);
    let r = resolve(&uses(&["o/inner@v2"]), &lock, &s, &f, Mode::Frozen, "L").unwrap();
    assert!(!r.changed && s.path("o/inner", B).is_file());
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn a_tarball_that_differs_from_the_lock_fails_the_run() {
    let (f, s) = (forge(), store());
    let r = resolve(&uses(&["o/inner@v2"]), &Lock::default(), &s, &f, Mode::Online, "L").unwrap();
    s.put("o/inner", B, &tar_of(&[("action.yml".into(), "runs:\n  using: node20\n  main: evil.js\n".into())])).unwrap();
    let e = resolve(&uses(&["o/inner@v2"]), &r.lock, &s, &f, Mode::Offline, "L").unwrap_err();
    assert!(e.contains("the tarball of `o/inner@v2`") && e.contains("but L has"), "{e}");
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn a_reference_with_no_action_file_is_refused() {
    let (f, s) = (forge(), store());
    let e = resolve(&uses(&["o/multi/other@main"]), &Lock::default(), &s, &f, Mode::Online, "L").unwrap_err();
    assert!(e.contains("no action.yml at `other`"), "{e}");
    let e = resolve(&uses(&["nothing"]), &Lock::default(), &s, &f, Mode::Online, "L").unwrap_err();
    assert!(e.contains("not OWNER/REPO"), "{e}");
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn the_workflow_gives_its_remote_actions_and_not_the_checkout() {
    let text = "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n      - uses: ./here\n      - uses: docker://i@sha256:00\n      - uses: o/r@v1\n";
    let w = pod_actions_workflow::parse(text).unwrap().workflow;
    assert_eq!(workflow_uses(&w), ["o/r@v1"]);
}

#[test]
fn a_checkout_of_another_repository_is_a_tree_to_pin() {
    let with = |text: &str| pod_actions_workflow::yaml::load(text, Limits::default()).unwrap();
    let t = |text: &str| checkout_target(Some(&with(text)), "me/app");
    assert_eq!(t("repository: o/lib\nref: v2\n").unwrap().as_deref(), Some("o/lib@v2"));
    assert_eq!(t("repository: o/lib\n").unwrap().as_deref(), Some("o/lib@HEAD"));
    assert_eq!(t("repository: Me/App\n").unwrap(), None, "the repository of the run is the local tree");
    assert_eq!(t("repository: ${{ github.repository }}\n").unwrap(), None);
    assert_eq!(t("path: x\n").unwrap(), None);
    assert!(t("repository: ${{ inputs.repo }}\n").unwrap_err().contains("plain"));
    assert!(t("repository: o/lib\nref: ${{ inputs.ref }}\n").unwrap_err().contains("plain"));
    assert!(t("repository: nothing\n").unwrap_err().contains("OWNER/REPO"));
    let text = "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n      - uses: actions/checkout@v4\n        with:\n          repository: o/lib\n          ref: v2\n";
    let w = pod_actions_workflow::parse(text).unwrap().workflow;
    assert_eq!(workflow_trees(&w, "me/app").unwrap(), ["o/lib@v2"]);
    // A tree is pinned and kept as an action is, with no action file.
    let (f, s) = (forge(), store());
    f.add("o/lib@v2", C, &[("README", "no action here")]);
    let r = add_trees(&["o/lib@v2".to_string()], Resolved::default(), &s, &f, Mode::Online, "L").unwrap();
    assert_eq!((r.commits["o/lib@v2"].as_str(), r.changed), (C, true));
    assert!(s.path("o/lib", C).is_file());
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn the_git_command_takes_the_proxy_of_the_grant_and_https_only() {
    let git = Git { base: "https://github.com/".into(), proxy: Some("http://127.0.0.1:9".into()) };
    let c = git.command(&["ls-remote", "https://github.com/a/b"], None);
    let env: std::collections::BTreeMap<String, Option<String>> = c
        .get_envs()
        .map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned())))
        .collect();
    let config: Vec<(String, String)> = (0..env["GIT_CONFIG_COUNT"].as_deref().unwrap().parse::<usize>().unwrap())
        .map(|i| {
            (
                env[&format!("GIT_CONFIG_KEY_{i}")].clone().unwrap(),
                env[&format!("GIT_CONFIG_VALUE_{i}")].clone().unwrap(),
            )
        })
        .collect();
    let has = |k: &str, v: &str| config.contains(&(k.to_string(), v.to_string()));
    assert!(has("http.proxy", "http://127.0.0.1:9"), "{config:?}");
    assert!(has("protocol.allow", "never") && has("protocol.https.allow", "always"), "{config:?}");
    assert!(has("fetch.recurseSubmodules", "false"), "{config:?}");
    assert_eq!(env["HTTPS_PROXY"], None, "the proxy of the environment is removed");
    let plain = Git { base: "http://git.example/".into(), proxy: None };
    let c = plain.command(&["ls-remote"], None);
    assert!(c.get_envs().any(|(k, v)| k == "GIT_CONFIG_KEY_1" && v.is_some_and(|v| v == "protocol.http.allow")));
    assert!(!c.get_envs().any(|(k, _)| k == "HTTPS_PROXY"), "no proxy: the environment stays");
}
