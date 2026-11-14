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
    let head = entries(&tarball(&dir, &scratch, Options { head: true, with_git: false }).unwrap());
    assert_eq!(head.get("a.txt").map(String::as_str), Some("committed"));
    assert!(!head.contains_key("new.txt"), "HEAD only: {head:?}");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn with_git_the_tree_has_a_git_directory_of_head() {
    let dir = repo("git");
    let scratch = dir.with_extension("scratch");
    let t = entries(&tarball(&dir, &scratch, Options { head: false, with_git: true }).unwrap());
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
    assert!(tarball(&plain, &scratch, Options { head: false, with_git: true }).unwrap_err().contains("not a git tree"));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&scratch);
    let _ = std::fs::remove_dir_all(&plain);
}
