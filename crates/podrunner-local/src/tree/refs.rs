//! The tree of a checkout of the same repository with `ref:` (T-640): the
//! commit of the ref in the local git, as GitHub gives it, not the working
//! tree. The run resolves each ref before its first job and keeps the
//! answer in its pins under `key`.

use std::path::{Path, PathBuf};

use pod_actions_workflow::{Kind, Node, Workflow};

/// The key of the pins of a run for `ref: R` of a checkout of the same
/// repository. Its value is the commit; empty for the commit of HEAD (the
/// local tree); `!` and the reason when the ref has no commit.
pub fn key(r: &str) -> String {
    format!("checkout-ref:{r}")
}

/// What `ref:` of a checkout of the same repository asks.
#[derive(Debug, PartialEq, Eq)]
pub enum Ask {
    /// No ref, or an empty one: the local tree.
    Default,
    /// An expression: the run cannot know its value before the job; the
    /// local tree stands for the commit of the run.
    Expression(String),
    /// A ref to resolve in the local git.
    Ref(String),
    /// A value that is no ref, with the reason.
    Bad(String),
}

/// The ask of the node of `ref:`.
pub fn ask(node: Option<&Node>) -> Ask {
    match node.map(|n| &n.kind) {
        None | Some(Kind::Null) => Ask::Default,
        Some(Kind::String(v)) if v.trim().is_empty() => Ask::Default,
        Some(Kind::String(v)) if v.contains("${{") => Ask::Expression(v.trim().to_string()),
        // An option of git, never a ref (T-631).
        Some(Kind::String(v)) if v.trim().starts_with('-') => {
            Ask::Bad(format!("`ref: {}` starts with -: not a ref", v.trim()))
        }
        Some(Kind::String(v)) => Ask::Ref(v.trim().to_string()),
        // A short commit id with only digits reads as a number in YAML.
        Some(Kind::Number(n)) if n.fract() == 0.0 && *n >= 0.0 && *n < 1e15 => Ask::Ref(format!("{}", *n as u64)),
        Some(_) => Ask::Bad("`ref:` is not text".into()),
    }
}

/// Each ref of a checkout of the same repository in the steps of `w`, with
/// the line of its step: the asks that the run resolves or names.
pub fn asks(w: &Workflow, self_repo: &str) -> Vec<(u32, Ask)> {
    let mut out = Vec::new();
    for s in w.jobs.iter().flat_map(|j| &j.steps) {
        let checkout = s.uses.as_deref().is_some_and(|u| u.starts_with("actions/checkout@"));
        if checkout && matches!(crate::store::checkout_target(s.with.as_ref(), self_repo), Ok(None)) {
            let a = ask(s.with.as_ref().and_then(|w| w.get("ref")));
            if a != Ask::Default {
                out.push((s.mark.line, a));
            }
        }
    }
    out
}

/// The commit of `r` in the git of `dir`. A ref that starts with `-` never
/// reaches git, and `--end-of-options` is the second guard.
pub fn resolve(dir: &Path, r: &str) -> Result<String, String> {
    if r.starts_with('-') {
        return Err(format!("`ref: {r}` starts with -: not a ref"));
    }
    if !super::is_git_tree(dir) {
        return Err(format!(
            "`ref: {r}`: {} is not a git tree, and the commit of a ref comes from its git",
            dir.display()
        ));
    }
    let spec = format!("{r}^{{commit}}");
    match rev_parse(&spec, &|args| super::git(dir, args)) {
        Ok(o) => Ok(String::from_utf8_lossy(&o).trim().to_string()),
        Err(_) => Err(format!(
            "`ref: {r}` is not a commit of the local git of {}; fix: git fetch origin {r}, or give another ref",
            dir.display()
        )),
    }
}

/// The answer of a call of git: its output, or its failure.
type Git = Result<Vec<u8>, String>;

/// `git rev-parse --verify` of `spec` through `git`. `--end-of-options` is
/// the second guard; `rev-parse` took it after git 2.24, and an older git
/// fails on it with no text (`--quiet`), so a failure tries again with no
/// guard. The first guard holds there: `spec` does not start with `-`.
fn rev_parse(spec: &str, git: &dyn Fn(&[&str]) -> Git) -> Git {
    let verify = ["rev-parse", "--verify", "--quiet"];
    git(&[&verify[..], &["--end-of-options", spec]].concat()).or_else(|_| git(&[&verify[..], &[spec]].concat()))
}

/// The commit of HEAD in the git of `dir`, if it has one.
pub fn head(dir: &Path) -> Option<String> {
    resolve(dir, "HEAD").ok()
}

/// Write the tarball of the files of `commit` (of the directory `dir` of
/// the tree) beside the tarball of the local tree in `scratch`, with the
/// files that the local tree leaves out left out, and with `with_git` a
/// `.git` at that commit. No submodule: the stand-in refuses them for a ref.
pub fn tarball(dir: &Path, scratch: &Path, commit: &str, o: &super::Options) -> Result<PathBuf, String> {
    std::fs::create_dir_all(scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    let out = scratch.join(podrunner_broker_fake::actions::checkout_file(commit));
    let fail = |e: std::io::Error| format!("{}: the tarball of {commit}: {e}", dir.display());
    let file = std::fs::File::create(&out).map_err(fail)?;
    let mut b = tar::Builder::new(flate2::write::GzEncoder::new(file, flate2::Compression::default()));
    b.follow_symlinks(false);
    let o = super::Options { head: true, ..o.clone() };
    // A checkout, not `git archive`: no `export-ignore`, no memory of the whole tar.
    let files = super::commit_tree(dir, scratch, commit)?;
    super::append_tree(&mut b, &files.root, Path::new(""), &o)?;
    drop(files);
    if o.with_git && super::is_git_top(dir) {
        let git = super::git_copy(dir, scratch)?;
        let clone = git.parent().expect("the clone").to_path_buf();
        // HEAD of the copy at the commit, detached, and its index.
        let at = super::git(&clone, &["update-ref", "--no-deref", "HEAD", commit]).and_then(|_| {
            super::git(&clone, &["reset", "-q"])?;
            b.append_dir_all(".git", &git).map_err(fail)
        });
        let _ = std::fs::remove_dir_all(&clone);
        at?;
    }
    b.into_inner().and_then(|gz| gz.finish()).map_err(fail)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// The files of a tarball and their text.
    fn files(path: &Path) -> std::collections::BTreeMap<String, String> {
        let mut a = tar::Archive::new(flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap()));
        let mut out = std::collections::BTreeMap::new();
        for e in a.entries().unwrap() {
            let mut e = e.unwrap();
            let name = e.path().unwrap().to_string_lossy().trim_start_matches("./").to_string();
            let mut text = String::new();
            let _ = e.read_to_string(&mut text);
            out.insert(name, text);
        }
        out
    }

    /// A repository with `main` and `feature` (HEAD), a tag on main, and a
    /// clone of it whose `origin/main` is a remote branch.
    #[test]
    fn a_ref_resolves_in_the_local_git_and_gives_its_files() {
        let base = std::env::temp_dir().join(format!("podrunner-tree-refs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("repo");
        std::fs::create_dir_all(&dir).unwrap();
        let g = |d: &Path, a: &[&str]| String::from_utf8(super::super::git(d, a).unwrap()).unwrap().trim().to_string();
        let commit = ["-c", "user.name=T", "-c", "user.email=t@example.invalid", "commit", "-q", "-m"];
        g(&dir, &["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("a.txt"), "main").unwrap();
        std::fs::write(dir.join(".secrets"), "S=1").unwrap();
        g(&dir, &["add", "-f", "a.txt", ".secrets"]);
        g(&dir, &[&commit[..], &["one"]].concat());
        g(&dir, &["tag", "v1"]);
        let main = g(&dir, &["rev-parse", "HEAD"]);
        g(&dir, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(dir.join("a.txt"), "feature").unwrap();
        g(&dir, &["add", "a.txt"]);
        g(&dir, &[&commit[..], &["two"]].concat());
        let feature = g(&dir, &["rev-parse", "HEAD"]);
        assert_eq!(head(&dir).as_deref(), Some(feature.as_str()));
        for r in ["main", "v1", "refs/heads/main", main.as_str(), &main[..7]] {
            assert_eq!(resolve(&dir, r).as_deref(), Ok(main.as_str()), "{r}");
        }
        assert_eq!(resolve(&dir, "feature").as_deref(), Ok(feature.as_str()));
        // A remote branch of a clone.
        let clone = base.join("clone");
        g(&base, &["clone", "-q", "--", dir.to_str().unwrap(), clone.to_str().unwrap()]);
        g(&clone, &["checkout", "-q", "-b", "work", "origin/feature"]);
        assert_eq!(resolve(&clone, "origin/main").as_deref(), Ok(main.as_str()));
        // No commit: the reason names the ref and the fix; an option never
        // reaches git.
        let e = resolve(&dir, "nosuch").unwrap_err();
        assert!(e.contains("`ref: nosuch` is not a commit") && e.contains("git fetch origin nosuch"), "{e}");
        let out = base.join("INJECT");
        let e = resolve(&dir, &format!("--output={}", out.display())).unwrap_err();
        assert!(e.contains("starts with -") && !out.exists(), "{e}");
        assert!(resolve(&dir, "").is_err() && resolve(&dir, "main..feature").is_err());
        // The tarball of main holds the files of main, not of the tree.
        let scratch = base.join("scratch");
        let t = tarball(&dir, &scratch, &main, &super::super::Options::default()).unwrap();
        assert_eq!(t, scratch.join(format!("tree-{main}.tar.gz")));
        let f = files(&t);
        assert_eq!(f.get("a.txt").map(String::as_str), Some("main"), "{f:?}");
        assert!(!f.contains_key(".secrets") && !f.keys().any(|k| k.starts_with(".git")), "{f:?}");
        // With git: HEAD of the copy is the commit.
        let o = super::super::Options { with_git: true, ..Default::default() };
        let t = tarball(&dir, &scratch, &main, &o).unwrap();
        let out = base.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let mut a = tar::Archive::new(flate2::read::GzDecoder::new(std::fs::File::open(&t).unwrap()));
        a.unpack(&out).unwrap();
        assert_eq!(g(&out, &["rev-parse", "HEAD"]), main);
        // The index fits the files; the secrets file stays out (T-276).
        assert_eq!(g(&out, &["status", "--porcelain"]), "D .secrets");
        assert!(!scratch.join("git-clone").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A git older than 2.31 fails `rev-parse` on `--end-of-options` with no
    /// text: the ref resolves with no guard of git (the first guard holds).
    #[test]
    fn an_older_git_resolves_a_ref_with_no_end_of_options() {
        let dir = std::env::temp_dir().join(format!("podrunner-tree-oldgit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let g = |a: &[&str]| String::from_utf8(super::super::git(&dir, a).unwrap()).unwrap().trim().to_string();
        g(&["init", "-q", "-b", "main"]);
        g(&["-c", "user.name=T", "-c", "user.email=t@example.invalid", "commit", "-q", "--allow-empty", "-m", "x"]);
        let head = g(&["rev-parse", "HEAD"]);
        let old = |args: &[&str]| match args.contains(&"--end-of-options") {
            true => Err("git: fatal".to_string()),
            false => super::super::git(&dir, args),
        };
        let got = rev_parse("main^{commit}", &old).unwrap();
        assert_eq!(String::from_utf8(got).unwrap().trim(), head);
        // A ref that is not there fails with the guard and without it.
        assert!(rev_parse("nosuch^{commit}", &old).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-640: the tree of a ref is the checkout of its commit. `git archive`
    /// applies `export-ignore` and `export-subst` of `.gitattributes`: the
    /// tree would miss files that GitHub gives and would change a text.
    #[test]
    fn the_tree_of_a_ref_ignores_the_export_attributes_of_the_commit() {
        let base = std::env::temp_dir().join(format!("podrunner-tree-attrs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("repo");
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let g = |d: &Path, a: &[&str]| String::from_utf8(super::super::git(d, a).unwrap()).unwrap().trim().to_string();
        g(&dir, &["init", "-q", "-b", "main"]);
        std::fs::write(dir.join(".gitattributes"), "docs export-ignore\ntest.sh export-ignore\nid.txt export-subst\n")
            .unwrap();
        std::fs::write(dir.join("docs/guide.md"), "guide").unwrap();
        std::fs::write(dir.join("test.sh"), "#!/bin/sh\n").unwrap();
        std::fs::write(dir.join("id.txt"), "$Format:%H$").unwrap();
        std::fs::write(dir.join("sub/s.txt"), "s").unwrap();
        std::os::unix::fs::symlink("s.txt", dir.join("sub/link")).unwrap();
        g(&dir, &["add", "-A"]);
        g(&dir, &["-c", "user.name=T", "-c", "user.email=t@example.invalid", "commit", "-q", "-m", "one"]);
        let main = g(&dir, &["rev-parse", "HEAD"]);
        let scratch = base.join("scratch");
        let all = files(&tarball(&dir, &scratch, &main, &super::super::Options::default()).unwrap());
        for f in ["docs/guide.md", "test.sh", ".gitattributes", "sub/s.txt"] {
            assert!(all.contains_key(f), "{f}: {:?}", all.keys());
        }
        assert_eq!(all.get("id.txt").map(String::as_str), Some("$Format:%H$"), "no export-subst");
        // A directory of the tree gets its own files, as with `git archive`.
        let sub = files(&tarball(&dir.join("sub"), &scratch, &main, &super::super::Options::default()).unwrap());
        assert_eq!(sub.keys().map(String::as_str).collect::<Vec<_>>(), ["link", "s.txt"], "{sub:?}");
        // The scratch space holds the tarball only.
        let left: Vec<_> = std::fs::read_dir(&scratch).unwrap().flatten().map(|e| e.file_name()).collect();
        assert!(left.iter().all(|n| n.to_string_lossy().ends_with(".tar.gz")), "{left:?}");
        // `--checkout-head` is the same tree.
        let head =
            super::super::tarball(&dir, &base.join("head"), super::super::Options { head: true, ..Default::default() })
                .unwrap();
        let head = files(&head);
        assert!(head.contains_key("docs/guide.md") && head.contains_key("test.sh"), "{:?}", head.keys());
        let _ = std::fs::remove_dir_all(&base);
    }
}
