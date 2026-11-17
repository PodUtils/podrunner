//! The checks of the paths that a run uses, before its first unit, also
//! in a dry run (T-345, T-356): a `planned` line is a promise that the run
//! can keep. A dry run writes no directory of the run.

use super::Args;

/// The first path of the run that the run could not use, by its flag.
pub(super) fn paths(a: &Args) -> Result<(), String> {
    if let Some(dir) = &a.runner_dir {
        if !dir.join("run.sh").is_file() || !dir.join("bin/Runner.Listener").is_file() {
            return Err(format!("--runner-dir {}: no unpacked runner (run.sh, bin/Runner.Listener)", dir.display()));
        }
        if let Some(e) = podrunner_local::exec::space_refusal(dir) {
            return Err(format!("--runner-dir {e}"));
        }
        // The run takes the directory with a lock file in it (#244).
        super::doctor::writable(dir)
            .map_err(|e| format!("--runner-dir {e}; the run writes its lock file .podrunner-run.lock there"))?;
    }
    let binaries = [
        a.guest.as_ref().map(|(microvm, b)| (if *microvm { "--microvm" } else { "--incus" }, b)),
        a.podnet.as_ref().map(|b| ("--podnet", b)),
        a.vnet.as_ref().map(|b| ("--vnet", b)),
    ];
    for (flag, b) in binaries.into_iter().flatten() {
        if !b.is_file() {
            return Err(format!("{flag} {}: not a file", b.display()));
        }
    }
    if let Some(t) = a.toolcache.as_ref().filter(|t| !t.is_dir()) {
        return Err(format!("--toolcache {}: not a directory; fix: mkdir -p {0}", t.display()));
    }
    // With `--offline` the store of actions is read only (#242).
    if let Some(d) = a.actions.as_ref().filter(|_| a.offline)
        && !d.is_dir()
    {
        return Err(format!("--actions {}: not a directory; --offline reads the actions from it", d.display()));
    }
    let dirs = [
        ("--actions", a.actions.as_ref().filter(|_| !a.offline)),
        ("--artifacts", a.artifacts.as_ref()),
        ("--cache", a.cache.as_ref().filter(|_| !a.no_cache)),
        ("--log-dir", a.log_dir.as_ref()),
        ("--export", a.export.as_ref()),
    ];
    for (flag, d) in dirs {
        if let Some(d) = d {
            super::doctor::writable(d).map_err(|e| format!("{flag} {e}"))?;
        }
    }
    Ok(())
}
