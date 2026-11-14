//! Actions from local directories (T-139): `--local-repository
//! OWNER/REPO@REF=DIR` serves the files of DIR as that reference, for the
//! work on an action. The commit is made up: the first 40 hex digits of the
//! SHA-256 of the tar, so a change of DIR gives a new commit. A local
//! reference is never in the lock, and the forge never sees it.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::store::Forge;

/// A forge that serves the local directories and asks `inner` for each
/// other reference.
pub struct Local<'a> {
    /// `OWNER/REPO@REF` and its directory.
    pub dirs: BTreeMap<String, PathBuf>,
    pub inner: &'a dyn Forge,
    tars: RefCell<BTreeMap<String, Vec<u8>>>,
}

impl<'a> Local<'a> {
    pub fn new(dirs: BTreeMap<String, PathBuf>, inner: &'a dyn Forge) -> Local<'a> {
        Local { dirs, inner, tars: RefCell::default() }
    }
}

/// The value of `--local-repository`: `OWNER/REPO@REF=DIR`.
pub fn parse(v: &str) -> Result<(String, PathBuf), String> {
    let bad = || format!("--local-repository {v:?} is not OWNER/REPO@REF=DIR");
    let (key, dir) = v.split_once('=').ok_or_else(bad)?;
    let (repo, r) = key.split_once('@').ok_or_else(bad)?;
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() != 2 || parts.iter().any(|p| p.is_empty()) || r.is_empty() || dir.is_empty() {
        return Err(bad());
    }
    Ok((key.to_string(), PathBuf::from(dir)))
}

/// The uncompressed tar of `dir` with one top directory, as the tarballs
/// of GitHub: in a git tree each tracked file and each new file that git
/// does not ignore; else each file.
fn tar_of(repo: &str, dir: &Path) -> Result<Vec<u8>, String> {
    let fail = |e: std::io::Error| format!("--local-repository {repo}: {}: {e}", dir.display());
    if !dir.is_dir() {
        return Err(format!("--local-repository {repo}: {} is not a directory", dir.display()));
    }
    let top = format!("{}-local", repo.replace('/', "-"));
    let mut b = tar::Builder::new(Vec::new());
    b.follow_symlinks(false);
    match crate::tree::is_git_tree(dir) {
        true => {
            for f in crate::tree::files(dir)? {
                b.append_path_with_name(dir.join(&f), Path::new(&top).join(&f)).map_err(fail)?;
            }
        }
        false => b.append_dir_all(&top, dir).map_err(fail)?,
    }
    b.into_inner().map_err(fail)
}

impl Forge for Local<'_> {
    fn resolve(&self, repo: &str, r: &str) -> Result<String, String> {
        let Some(dir) = self.dirs.get(&format!("{repo}@{r}")) else { return self.inner.resolve(repo, r) };
        let tar = tar_of(repo, dir)?;
        let commit = crate::store::digest(&tar)["sha256:".len()..][..40].to_string();
        self.tars.borrow_mut().insert(commit.clone(), tar);
        Ok(commit)
    }

    fn tar(&self, repo: &str, commit: &str) -> Result<Vec<u8>, String> {
        match self.tars.borrow().get(commit) {
            Some(t) => Ok(t.clone()),
            None => self.inner.tar(repo, commit),
        }
    }

    fn is_local(&self, key: &str) -> bool {
        self.dirs.contains_key(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::Lock;
    use crate::store::{Mode, Store, resolve};

    /// A forge that knows nothing: a local reference never reaches it.
    struct Nothing;
    impl Forge for Nothing {
        fn resolve(&self, repo: &str, r: &str) -> Result<String, String> {
            Err(format!("the forge was asked for {repo}@{r}"))
        }
        fn tar(&self, repo: &str, _: &str) -> Result<Vec<u8>, String> {
            Err(format!("the forge was asked for the tar of {repo}"))
        }
    }

    #[test]
    fn a_local_directory_is_served_as_an_action() {
        let base = std::env::temp_dir().join(format!("podrunner-localrepo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let action = base.join("action");
        std::fs::create_dir_all(action.join("sub")).unwrap();
        std::fs::write(
            action.join("sub/action.yml"),
            "runs:\n  using: composite\n  steps:\n    - run: echo hi\n      shell: sh\n",
        )
        .unwrap();
        let (key, dir) = parse(&format!("me/tool@main={}", action.display())).unwrap();
        let local = Local::new(BTreeMap::from([(key, dir)]), &Nothing);
        let store = Store { dir: base.join("store") };
        let r = resolve(&["me/tool/sub@main".into()], &Lock::default(), &store, &local, Mode::Frozen, "L").unwrap();
        let commit = r.commits["me/tool@main"].clone();
        assert_eq!(commit.len(), 40);
        assert!(r.lock.pins.is_empty() && !r.changed, "a local reference is not in the lock");
        assert!(store.path("me/tool", &commit).is_file(), "the broker serves it from the store");
        std::fs::write(action.join("sub/action.yml"), "runs:\n  using: composite\n  steps: []\n").unwrap();
        let again =
            resolve(&["me/tool/sub@main".into()], &Lock::default(), &store, &local, Mode::Offline, "L").unwrap();
        assert_ne!(again.commits["me/tool@main"], commit, "a change of the directory gives a new commit");
        for bad in ["me/tool@main", "tool@main=d", "me/tool=d", "me/tool@main="] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
