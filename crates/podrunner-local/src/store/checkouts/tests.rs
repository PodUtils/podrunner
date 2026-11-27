use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A store in the temp directory: it goes at the drop, also after a panic.
struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A tarball `OWNER_REPO/COMMIT.tar.gz` with the files `files` under one
/// top directory.
fn put(dir: &Path, repo: &str, commit: &str, files: &[(&str, &str)]) {
    let path = dir.join(pod_actions_store::archive_name(repo, commit));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let gz = flate2::write::GzEncoder::new(std::fs::File::create(&path).unwrap(), flate2::Compression::fast());
    let mut b = tar::Builder::new(gz);
    for (name, text) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(text.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        b.append_data(&mut h, format!("top/{name}"), text.as_bytes()).unwrap();
    }
    b.into_inner().unwrap().finish().unwrap();
}

fn composite(steps: &[&str]) -> String {
    format!("runs:\n  using: composite\n  steps:\n{}", steps.concat())
}

/// A checkout step with `with:` of `w`.
fn co(w: &str) -> String {
    format!("    - uses: actions/checkout@v4\n      with: {{{w}}}\n")
}

fn commit(n: u32) -> String {
    format!("{n:040x}")
}

/// T-718: the checkouts of a composite action of the store, of an action
/// in a path of its repository, of a nested action and of an action by a
/// full commit; a node action, a `docker://` step, an action that the
/// store does not have and a loop give none, and the walk ends. A `./`
/// step is a local action (T-725).
#[test]
fn the_checkouts_of_the_actions_of_the_store() {
    let dir = Dir(std::env::temp_dir().join(format!("podrunner-store-checkouts-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&dir.0);
    let d = dir.0.as_path();
    let a = composite(&[
        &co("repository: o/second, ref: v1"),
        "    - uses: o/nested@v2\n",
        "    - uses: ./local\n",
        "    - uses: docker://alpine:3\n",
        "    - uses: o/missing@v9\n",
        "    - uses: o/node@v1\n",
    ]);
    let sub = composite(&[&co("ref: v5")]);
    put(d, "o/act", &commit(1), &[("action.yml", &a), ("sub/action.yaml", &sub)]);
    let nested = composite(&[&co("repository: o/third"), "    - uses: o/act@v1\n"]);
    put(d, "o/nested", &commit(2), &[("action.yml", &nested)]);
    put(d, "o/node", &commit(3), &[("action.yml", "runs:\n  using: node20\n  main: x.js\n")]);
    put(d, "o/full", &commit(4), &[("action.yml", &composite(&[&co("")]))]);
    let commits: BTreeMap<String, String> =
        [("o/act@v1", 1), ("o/nested@v2", 2), ("o/node@v1", 3)].map(|(k, n)| (k.to_string(), commit(n))).into();
    let uses = ["o/act@v1", "o/act/sub@v1", &format!("o/full@{}", commit(4)), "o/none@v1"].map(String::from);
    let found = super::action_checkouts(&uses, d, &commits);
    // T-725: a `./` step names a local action of the workspace, one level
    // below its action.
    assert_eq!(found.locals, [("./local".to_string(), 2)]);
    let got = found.checkouts;
    let names: Vec<(&str, u32)> = got.iter().map(|(u, l, _)| (u.as_str(), *l)).collect();
    let full = format!("o/full@{}", commit(4));
    assert_eq!(names, [("o/act@v1", 4), ("o/act/sub@v1", 4), (full.as_str(), 4), ("o/nested@v2", 4)]);
    let target = |i: usize| crate::store::checkout_target(got[i].2.as_ref(), "me/self").unwrap();
    assert_eq!(target(0).as_deref(), Some("o/second@v1"));
    assert_eq!(target(1), None, "a ref of the same repository");
    assert_eq!(target(3).as_deref(), Some("o/third@HEAD"));
}

/// T-718: the walk follows composite actions to 10 levels, as GitHub.
#[test]
fn the_walk_ends_at_ten_levels() {
    let dir = Dir(std::env::temp_dir().join(format!("podrunner-store-depth-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&dir.0);
    let d = dir.0.as_path();
    let mut commits = BTreeMap::new();
    for n in 1..=12 {
        let next = format!("    - uses: o/a{}@v1\n", n + 1);
        put(d, &format!("o/a{n}"), &commit(n), &[("action.yml", &composite(&[&co("ref: v1"), &next]))]);
        commits.insert(format!("o/a{n}@v1"), commit(n));
    }
    let got = super::action_checkouts(&["o/a1@v1".to_string()], d, &commits).checkouts;
    let names: Vec<&str> = got.iter().map(|(u, _, _)| u.as_str()).collect();
    assert_eq!(names.len(), 10, "{names:?}");
    assert_eq!(names.last(), Some(&"o/a10@v1"));
}

/// T-726: a checkout by any case of the owner and the name in an action of
/// the store is a checkout, not an action to walk.
#[test]
fn a_checkout_by_any_case_in_an_action_of_the_store() {
    let dir = Dir(std::env::temp_dir().join(format!("podrunner-store-case-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&dir.0);
    let d = dir.0.as_path();
    let a =
        composite(&["    - uses: Actions/Checkout@v4\n      with: {ref: V1}\n", "    - uses: ACTIONS/CHECKOUT@v4\n"]);
    put(d, "o/act", &commit(1), &[("action.yml", &a)]);
    let commits = BTreeMap::from([("o/act@v1".to_string(), commit(1))]);
    let got = super::action_checkouts(&["o/act@v1".to_string()], d, &commits).checkouts;
    let lines: Vec<u32> = got.iter().map(|(_, l, _)| *l).collect();
    assert_eq!(lines, [4, 6]);
}

/// T-729: `$/PATH` in an action of the store names its repository at its
/// ref; `$/PATH@REF` names nothing.
#[test]
fn a_uses_of_its_own_repository_in_an_action_of_the_store() {
    let dir = Dir(std::env::temp_dir().join(format!("podrunner-store-self-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&dir.0);
    let d = dir.0.as_path();
    let a = composite(&["    - uses: $/sub\n", "    - uses: '$/bad@v1'\n"]);
    put(d, "o/act", &commit(1), &[("action.yml", &a), ("sub/action.yml", &composite(&[&co("ref: v5")]))]);
    let commits = BTreeMap::from([("o/act@v1".to_string(), commit(1))]);
    let got = super::action_checkouts(&["o/act@v1".to_string()], d, &commits).checkouts;
    let names: Vec<(&str, u32)> = got.iter().map(|(u, l, _)| (u.as_str(), *l)).collect();
    assert_eq!(names, [("o/act/sub@v1", 4)]);
}
