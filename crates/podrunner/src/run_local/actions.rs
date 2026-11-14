//! The actions of `podrunner run` by tag (T-118): each remote `uses:` of the
//! workflow, and of each composite action that it names, is pinned by the
//! lock file beside the workflow before the first job, and its tarball is
//! kept in the store (`--actions DIR`, by default the cache directory of the
//! user).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use podrunner_local::lock::{LOCK_FILE, Lock};
use podrunner_local::store::{Git, Mode, Store, resolve, workflow_uses};

/// The forge of the actions.
const FORGE: &str = "https://github.com/";

/// A directory of the cache of the user: `$XDG_CACHE_HOME/podrunner/NAME`,
/// or `$HOME/.cache/podrunner/NAME`.
pub(super) fn user_cache(name: &str) -> Result<PathBuf, String> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").filter(|v| !v.is_empty()).map(|h| PathBuf::from(h).join(".cache")))
        .ok_or_else(|| format!("no XDG_CACHE_HOME and no HOME: give the directory with --{name} DIR"))?;
    Ok(cache.join("podrunner").join(name))
}

/// The actions of a run.
pub(super) struct Pinned {
    /// The store of the tarballs.
    pub dir: PathBuf,
    /// The commit of each `OWNER/REPO@REF`.
    pub commits: BTreeMap<String, String>,
    /// A line for stderr when the lock changed.
    pub note: String,
}

/// The actions of the run, or `None` when the workflow names no action of a
/// repository.
pub(super) fn pin(file: &Path, text: &str, store: Option<&Path>, mode: Mode) -> Result<Option<Pinned>, String> {
    let workflow = pod_actions_workflow::parse(text).map_err(|e| e.to_string())?.workflow;
    let uses = workflow_uses(&workflow);
    if uses.is_empty() {
        return Ok(None);
    }
    let dir = match store {
        Some(d) => d.to_path_buf(),
        None => user_cache("actions")?,
    };
    let lock_path = file.parent().unwrap_or(Path::new(".")).join(LOCK_FILE);
    let lock = Lock::read(&lock_path)?;
    let name = lock_path.display().to_string();
    let r = resolve(&uses, &lock, &Store { dir: dir.clone() }, &Git { base: FORGE.into() }, mode, &name)?;
    let mut note = String::new();
    if r.changed {
        r.lock.write(&lock_path)?;
        note = format!("podrunner: {name} pins {} actions\n", r.lock.pins.len());
    }
    Ok(Some(Pinned { dir, commits: r.commits, note }))
}
