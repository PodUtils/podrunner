use std::collections::BTreeMap;
use std::io::Read;

use super::*;

/// Each file of a tarball and its text.
fn entries(path: &Path) -> BTreeMap<String, String> {
    let gz = flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap());
    let mut a = tar::Archive::new(gz);
    let mut out = BTreeMap::new();
    for e in a.entries().unwrap() {
        let mut e = e.unwrap();
        let name = e.path().unwrap().to_string_lossy().trim_start_matches("./").to_string();
        let mut text = String::new();
        let _ = e.read_to_string(&mut text);
        out.insert(name, text);
    }
    out
}

fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("podrunner-tree-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let g = |a: &[&str]| git(&dir, a).unwrap();
    g(&["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("a.txt"), "committed").unwrap();
    std::fs::write(dir.join(".gitignore"), "*.log\n").unwrap();
    g(&["add", "."]);
    g(&["-c", "user.name=T", "-c", "user.email=t@example.invalid", "commit", "-q", "-m", "one"]);
    std::fs::write(dir.join("a.txt"), "changed on disk").unwrap();
    std::fs::write(dir.join("new.txt"), "not committed").unwrap();
    std::fs::write(dir.join("debug.log"), "ignored").unwrap();
    dir
}

#[test]
fn the_tree_holds_the_changes_on_disk_and_not_the_ignored_files() {
    let dir = repo("disk");
    let scratch = dir.with_extension("scratch");
    let t = entries(&tarball(&dir, &scratch, Options::default()).unwrap());
    assert_eq!(t.get("a.txt").map(String::as_str), Some("changed on disk"));
    assert_eq!(t.get("new.txt").map(String::as_str), Some("not committed"));
    assert!(t.contains_key(".gitignore"));
    assert!(!t.contains_key("debug.log"), "an ignored file stays out: {t:?}");
    assert!(!t.keys().any(|k| k.starts_with(".git/")), "no .git by default");
    let head = entries(&tarball(&dir, &scratch, Options { head: true, ..Default::default() }).unwrap());
    assert_eq!(head.get("a.txt").map(String::as_str), Some("committed"));
    assert!(!head.contains_key("new.txt"), "HEAD only: {head:?}");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn with_git_the_tree_has_a_git_directory_of_head() {
    let dir = repo("git");
    let scratch = dir.with_extension("scratch");
    let t = entries(&tarball(&dir, &scratch, Options { with_git: true, ..Default::default() }).unwrap());
    assert!(t.contains_key(".git/HEAD") && t.contains_key(".git/index"), "{:?}", t.keys().collect::<Vec<_>>());
    // The copy unpacks to a tree where git reads HEAD.
    let out = scratch.join("unpacked");
    std::fs::create_dir_all(&out).unwrap();
    let gz = flate2::read::GzDecoder::new(std::fs::File::open(scratch.join("tree.tar.gz")).unwrap());
    tar::Archive::new(gz).unpack(&out).unwrap();
    let sha = String::from_utf8(git(&out, &["rev-parse", "HEAD"]).unwrap()).unwrap();
    assert_eq!(sha, String::from_utf8(git(&dir, &["rev-parse", "HEAD"]).unwrap()).unwrap());
    let status = String::from_utf8(git(&out, &["status", "--porcelain"]).unwrap()).unwrap();
    assert!(status.contains(" M a.txt") && status.contains("?? new.txt"), "{status}");
    let plain = std::env::temp_dir().join(format!("podrunner-tree-plain-{}", std::process::id()));
    std::fs::create_dir_all(&plain).unwrap();
    assert!(
        tarball(&plain, &scratch, Options { with_git: true, ..Default::default() })
            .unwrap_err()
            .contains("not a git tree")
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&scratch);
    let _ = std::fs::remove_dir_all(&plain);
}

/// T-253: a link comes after the file it names, also when its name sorts
/// first (`HomebrewFormula` of ripgrep): a tar with the link first fails on
/// the userland rung of podvm before 0.2.0-beta.24.
#[test]
fn a_link_comes_after_the_other_files() {
    let dir = repo("links");
    std::os::unix::fs::symlink("z-target", dir.join("a-link")).unwrap();
    std::fs::write(dir.join("z-target"), "target").unwrap();
    let out = tarball(&dir, &dir.join(".scratch"), Options::default()).unwrap();
    let gz = flate2::read::GzDecoder::new(std::fs::File::open(&out).unwrap());
    let names: Vec<String> = tar::Archive::new(gz)
        .entries()
        .unwrap()
        .map(|e| e.unwrap().path().unwrap().to_string_lossy().into_owned())
        .collect();
    let at = |n: &str| names.iter().position(|x| x == n).unwrap_or_else(|| panic!("{n}: {names:?}"));
    assert!(at("z-target") < at("a-link"), "{names:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The secrets of the run and a new lock file stay out of the tree of the
/// job (T-276, T-268), in a git tree and in a plain directory.
#[test]
fn the_secrets_and_the_lock_stay_out_of_the_tree() {
    let dir = repo("secrets");
    let scratch = dir.with_extension("scratch");
    std::fs::write(dir.join(".secrets"), "A=1").unwrap();
    std::fs::create_dir_all(dir.join(".github/workflows")).unwrap();
    std::fs::write(dir.join(".github/workflows/podrunner-actions.lock"), "lock").unwrap();
    std::fs::write(dir.join("my.env"), "B=2").unwrap();
    let o = Options { exclude: vec![PathBuf::from("my.env")], ..Default::default() };
    let t = entries(&tarball(&dir, &scratch, o.clone()).unwrap());
    assert!(t.contains_key("new.txt"), "{t:?}");
    for gone in [".secrets", ".github/workflows/podrunner-actions.lock", "my.env"] {
        assert!(!t.contains_key(gone), "{gone} in {t:?}");
    }
    // T-339 item 11: a committed lock stays, at its state on disk.
    let git = |args: &[&str]| {
        assert!(std::process::Command::new("git").arg("-C").arg(&dir).args(args).status().unwrap().success())
    };
    git(&["add", ".github/workflows/podrunner-actions.lock"]);
    git(&["-c", "user.name=t", "-c", "user.email=t@example.invalid", "commit", "-qm", "lock"]);
    std::fs::write(dir.join(".github/workflows/podrunner-actions.lock"), "lock, edited").unwrap();
    let t = entries(&tarball(&dir, &scratch, o.clone()).unwrap());
    assert!(t.contains_key(".github/workflows/podrunner-actions.lock"), "{t:?}");
    let plain = dir.with_extension("plain");
    let _ = std::fs::remove_dir_all(&plain);
    std::fs::create_dir_all(plain.join("empty")).unwrap();
    std::fs::write(plain.join(".secrets"), "A=1").unwrap();
    std::fs::write(plain.join("my.env"), "B=2").unwrap();
    std::fs::write(plain.join("kept.txt"), "k").unwrap();
    let t = entries(&tarball(&plain, &scratch, o).unwrap());
    assert!(t.contains_key("kept.txt") && t.keys().any(|k| k.trim_end_matches('/') == "empty"), "{t:?}");
    assert!(!t.contains_key(".secrets") && !t.contains_key("my.env"), "{t:?}");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&plain);
    let _ = std::fs::remove_dir_all(&scratch);
}

/// #220: a directory in a git tree is not its top: no `.git` by default, and
/// `--with-git` is refused with the fix.
#[test]
fn a_subdirectory_of_a_git_tree_gets_no_git() {
    let dir = repo("subdir");
    let scratch = dir.with_extension("scratch");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/s.txt"), "s").unwrap();
    assert!(is_git_top(&dir) && !is_git_top(&dir.join("sub")));
    let o = Options { with_git: true, ..Default::default() };
    let e = tarball(&dir.join("sub"), &scratch, o).unwrap_err();
    assert!(e.contains("give the top of the tree"), "{e}");
    let t = entries(&tarball(&dir.join("sub"), &scratch, Options::default()).unwrap());
    assert!(t.contains_key("s.txt") && !t.keys().any(|k| k.starts_with(".git")), "{t:?}");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&scratch);
}

/// T-425: the tarball holds the files of each submodule with a work tree,
/// to each level, and the list of the submodules; a submodule with no work
/// tree is in the list with no files.
#[test]
fn the_files_and_the_list_of_the_submodules() {
    let base = std::env::temp_dir().join(format!("podrunner-tree-subs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let commit = |d: &Path| {
        git(d, &["add", "."]).unwrap();
        git(d, &["-c", "user.name=T", "-c", "user.email=t@example.invalid", "commit", "-q", "-m", "c"]).unwrap();
    };
    let init = |name: &str, file: &str| {
        let d = base.join(name);
        std::fs::create_dir_all(&d).unwrap();
        git(&d, &["init", "-q", "-b", "main"]).unwrap();
        std::fs::write(d.join(file), name).unwrap();
        commit(&d);
        d
    };
    let add = |to: &Path, from: &Path, at: &str| {
        let url = from.display().to_string();
        git(to, &["-c", "protocol.file.allow=always", "submodule", "add", "-q", &url, at]).unwrap();
        commit(to);
    };
    let inner = init("inner", "deep.txt");
    let lib = init("lib", "lib.txt");
    add(&lib, &inner, "nested");
    let top = init("top", "top.txt");
    add(&top, &lib, "vendor/lib");
    git(&top, &["-c", "protocol.file.allow=always", "submodule", "update", "-q", "--init", "--recursive"]).unwrap();
    for head in [false, true] {
        let t = tarball(&top, &base.join(format!("scratch-{head}")), Options { head, ..Options::default() }).unwrap();
        let e = entries(&t);
        assert_eq!(e.get("vendor/lib/lib.txt").map(String::as_str), Some("lib"), "head {head}: {:?}", e.keys());
        assert_eq!(e.get("vendor/lib/nested/deep.txt").map(String::as_str), Some("inner"), "head {head}");
        assert_eq!(e[SUBMODULES], "1 1 vendor/lib\n2 1 vendor/lib/nested\n", "head {head}");
    }
    // A clone with no `submodule update`: the submodule has no work tree.
    let bare = base.join("clone");
    git(&base, &["clone", "-q", &top.display().to_string(), &bare.display().to_string()]).unwrap();
    let e = entries(&tarball(&bare, &base.join("scratch-clone"), Options::default()).unwrap());
    assert_eq!(e[SUBMODULES], "1 0 vendor/lib\n");
    assert!(!e.keys().any(|k| k.starts_with("vendor/lib/")), "{:?}", e.keys());
    let _ = std::fs::remove_dir_all(&base);
}
