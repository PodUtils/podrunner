//! The tree that the stand-in of `actions/checkout` gives to a job (T-092,
//! T-133). In a git tree it holds each tracked file and each new file that
//! git does not ignore, at its state on disk, as the user sees the tree;
//! `head` gives the committed files of HEAD only. `with_git` adds a `.git`:
//! a clone of the tree with no checkout and an index of HEAD, so that `git`
//! works in a step and the unit never writes the `.git` of the user. Out of
//! a git tree, it holds each file of the directory.

use std::path::{Path, PathBuf};
use std::process::Command;

/// What the tarball holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// The committed files of HEAD only (`--checkout-head`).
    pub head: bool,
    /// A `.git` of the tree (`--with-git`).
    pub with_git: bool,
}

fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(out.stdout)
}

pub(crate) fn is_git_tree(dir: &Path) -> bool {
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
    git(Path::new("."), &["clone", "-q", "--no-checkout", "--no-hardlinks", &top, &target])?;
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
        true => list = files(dir).unwrap_or_default(),
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

/// Write the tarball of the tree `dir` into `scratch`; its path.
pub fn tarball(dir: &Path, scratch: &Path, o: Options) -> Result<PathBuf, String> {
    std::fs::create_dir_all(scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    let out = scratch.join("tree.tar.gz");
    let fail = |e: std::io::Error| format!("{}: the tarball of the tree: {e}", dir.display());
    let file = std::fs::File::create(&out).map_err(fail)?;
    let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut b = tar::Builder::new(gz);
    b.follow_symlinks(false);
    if !is_git_tree(dir) {
        b.append_dir_all(".", dir).map_err(fail)?;
    } else if o.head {
        let head = git(dir, &["archive", "--format=tar", "HEAD"])?;
        let mut a = tar::Archive::new(head.as_slice());
        for entry in a.entries().map_err(fail)? {
            let mut entry = entry.map_err(fail)?;
            let mut h = entry.header().clone();
            let path = entry.path().map_err(fail)?.into_owned();
            b.append_data(&mut h, path, &mut entry).map_err(fail)?;
        }
    } else {
        for f in files(dir)? {
            b.append_path_with_name(dir.join(&f), &f).map_err(|e| format!("{}: {e}", dir.join(&f).display()))?;
        }
    }
    if o.with_git {
        if !is_git_tree(dir) {
            return Err(format!("--with-git: {} is not a git tree", dir.display()));
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
