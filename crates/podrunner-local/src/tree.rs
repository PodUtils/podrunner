//! The tree that the stand-in of `actions/checkout` gives to a job (T-092,
//! T-133). In a git tree it holds each tracked file and each new file that
//! git does not ignore, at its state on disk, as the user sees the tree;
//! `head` gives the committed files of HEAD only. `with_git` adds a `.git`:
//! a clone of the tree with no checkout and an index of HEAD, so that `git`
//! works in a step and the unit never writes the `.git` of the user. Out of
//! a git tree, it holds each file of the directory.

use std::path::{Path, PathBuf};
use std::process::Command;

mod submodules;

/// The tree of a commit of the local git, for `ref:` of a checkout (T-640).
pub mod ignored;
pub mod refs;

/// The list of the submodules in the tarball (T-425).
pub use submodules::LIST as SUBMODULES;

/// The levels of `submodules: recursive`.
pub const SUBMODULE_LEVELS: u32 = 10;

/// What the tarball holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// The committed files of HEAD only (`--checkout-head`).
    pub head: bool,
    /// A `.git` of the tree (`--with-git`).
    pub with_git: bool,
    /// More files to leave out, relative to the tree: the files of the
    /// run that hold secrets or values (`--secret-file`, `--env-file`).
    pub exclude: Vec<PathBuf>,
}

/// The default secrets file of a tree: podrunner reads it, and no step may.
pub const SECRETS: &str = ".secrets";

/// A file of podrunner that the tarball leaves out: the secrets of the run
/// would be plain text in the workspace (T-276), and a new lock file would
/// make the tree of the job unclean (T-268).
/// With `--checkout-head`, a committed lock file stays: the user chose it.
fn left_out(rel: &Path, o: &Options) -> bool {
    let rel = rel.strip_prefix(".").unwrap_or(rel);
    rel == Path::new(SECRETS)
        || (!o.head && rel.file_name().is_some_and(|n| n == crate::lock::LOCK_FILE))
        || o.exclude.iter().any(|e| e == rel)
}

/// Each directory and file under `dir` (not `.git`), relative, directories
/// first.
fn plain_files(dir: &Path, rel: &Path, out: &mut Vec<(PathBuf, bool)>) {
    let Ok(entries) = std::fs::read_dir(dir.join(rel)) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let r = rel.join(e.file_name());
        match e.file_type() {
            Ok(t) if t.is_dir() => {
                out.push((r.clone(), true));
                plain_files(dir, &r, out);
            }
            Ok(_) => out.push((r, false)),
            Err(_) => {}
        }
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    git_with(dir, args, &[])
}

/// [`git`] with more variables in its environment.
fn git_with(dir: &Path, args: &[&str], env: &[(&str, &std::ffi::OsStr)]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .envs(env.iter().copied())
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(out.stdout)
}

/// The files of a commit, written into a directory of the scratch space;
/// the directory goes with the value.
pub(crate) struct CommitTree {
    base: PathBuf,
    /// The directory of the checkout `dir` in it: the whole tree, or its
    /// subdirectory when `dir` is one.
    pub root: PathBuf,
}

impl Drop for CommitTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// Write the files of `commit` (`HEAD`, or a full commit id) of the git that
/// holds `dir` into `scratch`, as `git checkout` writes them, through a
/// temporary index (the index of the user stays). `git archive` is not used:
/// it applies `export-ignore` and `export-subst` of `.gitattributes`, so its
/// tree can differ from the checkout of GitHub, and it gives the whole tar in
/// memory (T-640). A subdirectory of the tree gets its own files, as it does
/// with `git archive`.
pub(crate) fn commit_tree(dir: &Path, scratch: &Path, commit: &str) -> Result<CommitTree, String> {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let hex = commit.bytes().all(|b| b.is_ascii_hexdigit());
    if commit != "HEAD" && !(hex && (7..=64).contains(&commit.len())) {
        return Err(format!("{commit:?} is not a commit id"));
    }
    std::fs::create_dir_all(scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = scratch
        .canonicalize()
        .map_err(|e| format!("{}: {e}", scratch.display()))?
        .join(format!("commit-tree-{}-{n}", std::process::id()));
    let mut tree = CommitTree { root: base.join("files"), base };
    std::fs::create_dir_all(&tree.root).map_err(|e| format!("{}: {e}", tree.root.display()))?;
    let index = tree.base.join("index");
    let env: [(&str, &std::ffi::OsStr); 1] = [("GIT_INDEX_FILE", index.as_os_str())];
    // A sparse checkout of the user does not shrink the commit.
    git_with(dir, &["-c", "core.sparseCheckout=false", "read-tree", commit], &env)?;
    git_with(dir, &["checkout-index", "-a", "-f", "-q", &format!("--prefix={}/", tree.root.display())], &env)?;
    let prefix = String::from_utf8_lossy(&git(dir, &["rev-parse", "--show-prefix"])?).trim().to_string();
    tree.root = tree.root.join(prefix);
    Ok(tree)
}

/// Add each directory and file under `root` to `b` under `prefix`, except
/// the files that [`left_out`] names; each link after the other files.
fn append_tree<W: std::io::Write>(
    b: &mut tar::Builder<W>,
    root: &Path,
    prefix: &Path,
    o: &Options,
) -> Result<(), String> {
    let mut all = Vec::new();
    plain_files(root, Path::new(""), &mut all);
    let mut list = Vec::new();
    let fail = |e: std::io::Error| format!("{}: {e}", root.display());
    for (f, is_dir) in all.into_iter().filter(|(f, _)| !left_out(&prefix.join(f), o)) {
        match is_dir {
            true => b.append_dir(prefix.join(&f), root.join(&f)).map_err(fail)?,
            false => list.push(f),
        }
    }
    for f in links_last(root, list) {
        b.append_path_with_name(root.join(&f), prefix.join(&f)).map_err(fail)?;
    }
    Ok(())
}

/// Whether `dir` is the top of a git tree: only a checkout of the top gets
/// a `.git` whose index fits the files of the job (#220).
pub fn is_git_top(dir: &Path) -> bool {
    let top = git(dir, &["rev-parse", "--show-toplevel"]).map(|o| String::from_utf8_lossy(&o).trim().to_string());
    match (top, dir.canonicalize()) {
        (Ok(top), Ok(dir)) => Path::new(&top).canonicalize().is_ok_and(|t| t == dir),
        _ => false,
    }
}

pub fn is_git_tree(dir: &Path) -> bool {
    git(dir, &["rev-parse", "--is-inside-work-tree"]).is_ok_and(|o| o.starts_with(b"true"))
}

/// The files of the tree: tracked and new, not ignored, that exist on disk.
pub(crate) fn files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let out = git(dir, &["ls-files", "-z", "--cached", "--others", "--exclude-standard"])?;
    let mut list: Vec<PathBuf> = out
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| PathBuf::from(String::from_utf8_lossy(p).into_owned()))
        .filter(|p| std::fs::symlink_metadata(dir.join(p)).is_ok())
        .collect();
    list.sort();
    list.dedup();
    Ok(list)
}

/// A `.git` for the job: a clone of `dir` with no checkout, its index reset
/// to HEAD, in `scratch`.
fn git_copy(dir: &Path, scratch: &Path) -> Result<PathBuf, String> {
    let clone = scratch.join("git-clone");
    let _ = std::fs::remove_dir_all(&clone);
    let top = String::from_utf8_lossy(&git(dir, &["rev-parse", "--show-toplevel"])?).trim().to_string();
    let target = clone.display().to_string();
    // `--`: a path never reads as an option (T-631).
    git(Path::new("."), &["clone", "-q", "--no-checkout", "--no-hardlinks", "--", &top, &target])?;
    git(&clone, &["reset", "-q"])?;
    Ok(clone.join(".git"))
}

/// A stamp of the files of the tree `dir` (the files of the tarball: in a
/// git tree the tracked and new files that git does not ignore): their
/// names, sizes and times of change. A change of a file changes it
/// (T-142).
pub fn stamp(dir: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    fn walk(dir: &Path, rel: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir.join(rel)) else { return };
        for e in entries.flatten() {
            let r = rel.join(e.file_name());
            match e.file_type() {
                Ok(t) if t.is_dir() && e.file_name() != ".git" => walk(dir, &r, out),
                Ok(t) if !t.is_dir() => out.push(r),
                _ => {}
            }
        }
    }
    let mut list = Vec::new();
    match is_git_tree(dir) {
        true => {
            list = files(dir).unwrap_or_default();
            list.extend(submodules::files(dir).unwrap_or_default());
        }
        false => walk(dir, Path::new(""), &mut list),
    }
    list.sort();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for f in &list {
        f.hash(&mut h);
        if let Ok(m) = std::fs::symlink_metadata(dir.join(f)) {
            m.len().hash(&mut h);
            m.modified().ok().hash(&mut h);
        }
    }
    h.finish()
}

/// `files` of `dir` with each symbolic link after the other files: a tar
/// that has a link before its target fails on some hosts (`utime` through
/// the link: podvm before 0.2.0-beta.24, T-232, the user-path gate).
pub fn links_last(dir: &Path, files: Vec<PathBuf>) -> Vec<PathBuf> {
    let (links, other): (Vec<PathBuf>, Vec<PathBuf>) = files
        .into_iter()
        .partition(|f| std::fs::symlink_metadata(dir.join(f)).is_ok_and(|m| m.file_type().is_symlink()));
    other.into_iter().chain(links).collect()
}

/// Write the tarball of the tree `dir` into `scratch`; its path.
pub fn tarball(dir: &Path, scratch: &Path, o: Options) -> Result<PathBuf, String> {
    std::fs::create_dir_all(scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    let out = scratch.join("tree.tar.gz");
    let fail = |e: std::io::Error| format!("{}: the tarball of the tree: {e}", dir.display());
    let file = std::fs::File::create(&out).map_err(fail)?;
    let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut b = tar::Builder::new(gz);
    b.follow_symlinks(false);
    let add = |b: &mut tar::Builder<_>, f: &Path| {
        b.append_path_with_name(dir.join(f), f).map_err(|e| format!("{}: {e}", dir.join(f).display()))
    };
    if !is_git_tree(dir) {
        if !dir.is_dir() {
            return Err(format!("{}: not a directory", dir.display()));
        }
        append_tree(&mut b, dir, Path::new(""), &o)?;
    } else if o.head {
        {
            let head = commit_tree(dir, scratch, "HEAD")?;
            append_tree(&mut b, &head.root, Path::new(""), &o)?;
        }
        // The files of each submodule at the commit that HEAD records.
        for s in submodules::list(dir, true)?.iter().filter(|s| s.ready) {
            let sub = commit_tree(&dir.join(&s.path), scratch, &s.commit).map_err(|e| {
                format!("the submodule {}: {e}; fix: git submodule update --init --recursive", s.path.display())
            })?;
            append_tree(&mut b, &sub.root, &s.path, &o)?;
        }
    } else {
        // A committed lock stays at its state on disk, as each tracked
        // file: leaving it out made it deleted in the job (T-339 item 11).
        let out = git(dir, &["ls-files", "-z", "--cached"])?;
        let tracked: std::collections::BTreeSet<PathBuf> = out
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| PathBuf::from(String::from_utf8_lossy(p).into_owned()))
            .collect();
        let lock = |f: &Path| f.file_name().is_some_and(|n| n == crate::lock::LOCK_FILE) && tracked.contains(f);
        let mut list: Vec<PathBuf> =
            files(dir)?.into_iter().filter(|f| !left_out(f, &o) || (lock(f) && !o.exclude.contains(f))).collect();
        // The files of each submodule with a work tree, at their state on
        // disk (T-425).
        if !submodules::list(dir, false)?.is_empty() {
            list.extend(submodules::files(dir)?.into_iter().filter(|f| !left_out(f, &o)));
            list.sort();
            list.dedup();
        }
        for f in links_last(dir, list) {
            add(&mut b, &f)?;
        }
    }
    // The list of the submodules, for the stand-in (T-425).
    if is_git_tree(dir) {
        let text = submodules::list_text(&submodules::list(dir, o.head)?);
        if !text.is_empty() {
            let mut h = tar::Header::new_gnu();
            h.set_size(text.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, SUBMODULES, text.as_bytes()).map_err(fail)?;
        }
    }
    if o.with_git {
        if !is_git_tree(dir) {
            return Err(format!("--with-git: {} is not a git tree", dir.display()));
        }
        if !is_git_top(dir) {
            return Err(format!(
                "--with-git: {} is a directory in a git tree, and its .git would hold the whole tree; fix: give the top of the tree to --checkout, or leave out --with-git",
                dir.display()
            ));
        }
        let copy = git_copy(dir, scratch)?;
        b.append_dir_all(".git", &copy).map_err(fail)?;
        let _ = std::fs::remove_dir_all(copy.parent().expect("the clone"));
    }
    b.into_inner().and_then(|gz| gz.finish()).map_err(fail)?;
    Ok(out)
}

#[cfg(test)]
mod tests;
