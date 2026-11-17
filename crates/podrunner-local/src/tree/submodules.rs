//! The git submodules of the local tree (T-425). The tarball holds the
//! files of each submodule that has a work tree, and a list of the
//! submodules (`LIST`): the stand-in of `actions/checkout` keeps those of
//! its `submodules:` (`false`: none, `true`: the first level,
//! `recursive`: each level) and leaves each other one an empty directory,
//! as GitHub does. A submodule with no work tree (not initialized) has no
//! files: a checkout that asks for it fails with the fix.

use std::path::{Path, PathBuf};

use super::git;

/// The list of the submodules in the tarball: one line `DEPTH READY PATH`
/// for each submodule, READY 1 when it has files. The stand-in reads it,
/// then removes it.
pub const LIST: &str = ".podrunner-submodules.list";

/// One submodule of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sub {
    /// Its path, relative to the tree.
    pub path: PathBuf,
    /// 1 for a submodule of the tree, 2 for a submodule of it, and so on.
    pub depth: u32,
    /// It has a work tree with files.
    pub ready: bool,
    /// The commit that its parent records (at HEAD for `--checkout-head`,
    /// else in the index).
    pub commit: String,
}

/// The gitlinks of the git tree `dir`: of the commit `rev`, or of the index.
fn links(dir: &Path, rev: Option<&str>) -> Result<Vec<(PathBuf, String)>, String> {
    let out = match rev {
        Some(r) => git(dir, &["ls-tree", "-r", "-z", r])?,
        None => git(dir, &["ls-files", "-z", "-s"])?,
    };
    Ok(out
        .split(|b| *b == 0)
        .filter_map(|e| {
            let (meta, path) = std::str::from_utf8(e).ok()?.split_once('\t')?;
            let mut f = meta.split(' ');
            let mode = f.next()?;
            // ls-tree: MODE TYPE SHA; ls-files -s: MODE SHA STAGE.
            let sha = if rev.is_some() { f.nth(1)? } else { f.next()? };
            (mode == "160000").then(|| (PathBuf::from(path), sha.to_string()))
        })
        .collect())
}

/// Each submodule of the git tree `top`, parents before their children:
/// at HEAD with `head`, else as the index records it.
pub fn list(top: &Path, head: bool) -> Result<Vec<Sub>, String> {
    let mut out = Vec::new();
    walk(top, Path::new(""), head.then_some("HEAD"), 1, &mut out)?;
    Ok(out)
}

fn walk(top: &Path, rel: &Path, rev: Option<&str>, depth: u32, out: &mut Vec<Sub>) -> Result<(), String> {
    if depth > super::SUBMODULE_LEVELS {
        return Ok(());
    }
    for (p, commit) in links(&top.join(rel), rev)? {
        let path = rel.join(p);
        let ready = top.join(&path).join(".git").exists();
        out.push(Sub { path: path.clone(), depth, ready, commit: commit.clone() });
        if ready {
            walk(top, &path, rev.map(|_| commit.as_str()), depth + 1, out)?;
        }
    }
    Ok(())
}

/// The files of the work trees of the submodules of `top`, tracked, at
/// their state on disk, relative to `top`.
pub fn files(top: &Path) -> Result<Vec<PathBuf>, String> {
    let out = git(top, &["ls-files", "-z", "--cached", "--recurse-submodules"])?;
    Ok(out
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| PathBuf::from(String::from_utf8_lossy(p).into_owned()))
        .filter(|p| std::fs::symlink_metadata(top.join(p)).is_ok_and(|m| !m.is_dir()))
        .collect())
}

/// The tar of the files of the submodule `s` at its recorded commit, with
/// its path as the prefix (`--checkout-head`).
pub fn archive(top: &Path, s: &Sub) -> Result<Vec<u8>, String> {
    let prefix = format!("{}/", s.path.display());
    git(&top.join(&s.path), &["archive", "--format=tar", &format!("--prefix={prefix}"), &s.commit])
        .map_err(|e| format!("the submodule {}: {e}; fix: git submodule update --init --recursive", s.path.display()))
}

/// The text of [`LIST`]. A path with a line end is left out: the stand-in
/// reads one line for each submodule.
pub fn list_text(subs: &[Sub]) -> String {
    subs.iter()
        .filter_map(|s| s.path.to_str().filter(|p| !p.contains('\n')).map(|p| (s, p)))
        .map(|(s, p)| format!("{} {} {p}\n", s.depth, u8::from(s.ready)))
        .collect()
}
