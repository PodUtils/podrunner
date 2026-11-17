//! The units of a run that died (T-312): a SIGKILL of podrunner leaves its
//! podvm units running and its directories `podrunner-run-units-PID` and
//! `podrunner-run-tree-PID` in `TMPDIR`. The next run of the same user
//! ends each process of those units (by the tag of each unit, as `destroy`
//! finds them) and removes the directories, and says so.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

const PREFIXES: &[&str] = &["podrunner-run-units-", "podrunner-run-tree-"];

/// The directories of this run: each one is removed when the run returns.
pub(super) struct Gone(pub(super) Vec<PathBuf>);

impl Drop for Gone {
    fn drop(&mut self) {
        for d in &self.0 {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

/// The temporary directory of the run: it must exist and take a
/// directory, or each later step fails with a path only (T-323 F14).
fn tmp_ok(tmp: &Path) -> Result<(), String> {
    let probe = tmp.join(format!("podrunner-probe-{}", std::process::id()));
    std::fs::create_dir(&probe)
        .map(|()| drop(std::fs::remove_dir(&probe)))
        .map_err(|e| format!("TMPDIR {}: {e}: set TMPDIR to a directory that you can write", tmp.display()))
}

/// The tree directory and the units directory of the run `pid` under
/// `tmp`, each new at 0700 (T-407, #250): a name that another user made
/// first, or a link, is refused, so no write of the run follows it.
/// A `tmp` that cannot take a directory is named first (T-323 F14).
pub(super) fn run_dirs(tmp: &Path, pid: u32) -> Result<(PathBuf, PathBuf), String> {
    tmp_ok(tmp)?;
    let (tree, units) = (tmp.join(format!("{}{pid}", PREFIXES[1])), tmp.join(format!("{}{pid}", PREFIXES[0])));
    podrunner_local::work::private_dir(&tree)?;
    podrunner_local::work::private_dir(&units)?;
    Ok((tree, units))
}

/// Whether a process `pid` lives. Any process counts, not only one named
/// podrunner: a run started through a link of another name stays safe.
fn live_podrunner(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// The directories of dead runs of this user in `tmp`.
pub(super) fn dead_runs(tmp: &Path, uid: u32, me: u32) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(tmp) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let pid: u32 = PREFIXES.iter().find_map(|p| name.strip_prefix(p))?.parse().ok()?;
            let meta = std::fs::symlink_metadata(e.path()).ok()?;
            (meta.is_dir() && meta.uid() == uid && pid != me && !live_podrunner(pid)).then(|| e.path())
        })
        .collect()
}

/// Each `(state, unit)` of a units directory: `STATE/units/UNIT` at any
/// depth up to three (the routes and the slots of a run).
fn units_of(dir: &Path, depth: u32, out: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())) {
        if e.file_name() == "units" {
            for u in std::fs::read_dir(e.path()).into_iter().flatten().flatten() {
                out.push((dir.to_path_buf(), u.file_name().to_string_lossy().into_owned()));
            }
        } else if depth > 0 {
            units_of(&e.path(), depth - 1, out);
        }
    }
}

/// End the units of each dead run and remove its directories.
/// With `dry`, a dry run, nothing ends and nothing goes (#237).
pub(super) fn end_dead_runs(dry: bool) {
    if dry {
        return;
    }
    // SAFETY: getuid has no failure.
    let uid = unsafe { libc::getuid() };
    for dir in dead_runs(&std::env::temp_dir(), uid, std::process::id()) {
        let mut units = Vec::new();
        units_of(&dir, 3, &mut units);
        let mut ended = 0;
        for (state, unit) in &units {
            for pid in podrunner_unit::procs::tagged(&podrunner_unit::procs::tag_of(state, unit)) {
                podrunner_unit::procs::kill(pid, libc::SIGKILL);
                ended += 1;
            }
        }
        let what = match units.len() {
            0 => String::new(),
            n => format!(" ({n} units, {ended} processes ended)"),
        };
        match podrunner_local::work::clear(&dir) {
            Ok(()) => eprintln!("podrunner: {}: the files of a run that died are gone{what}", dir.display()),
            Err(e) => eprintln!("podrunner: a run that died left {}: {e}", dir.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #237: a dry run leaves the directories of a dead run.
    #[test]
    fn a_dry_run_leaves_a_dead_run() {
        let dead = std::env::temp_dir().join("podrunner-run-units-4194307");
        std::fs::create_dir_all(dead.join("default/units/u-1")).unwrap();
        end_dead_runs(true);
        assert!(dead.is_dir(), "a dry run removed the files of a dead run");
        let _ = std::fs::remove_dir_all(&dead);
    }

    /// A directory of a dead run is found; one of a live podrunner, of
    /// this process or of another name is not.
    #[test]
    fn the_directories_of_dead_runs_are_found() {
        let tmp = std::env::temp_dir().join(format!("podrunner-stale-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        // A pid that no process has: above the maximum of the kernel.
        let dead = tmp.join("podrunner-run-units-4194305");
        std::fs::create_dir_all(dead.join("default/units/u-1")).unwrap();
        std::fs::create_dir_all(tmp.join("podrunner-run-tree-4194306")).unwrap();
        std::fs::create_dir_all(tmp.join(format!("podrunner-run-units-{}", std::process::id()))).unwrap();
        std::fs::create_dir_all(tmp.join("other-4194305")).unwrap();
        // SAFETY: getuid has no failure.
        let uid = unsafe { libc::getuid() };
        let mut got = dead_runs(&tmp, uid, std::process::id());
        got.sort();
        assert_eq!(got, [tmp.join("podrunner-run-tree-4194306"), dead.clone()]);
        assert!(dead_runs(&tmp, uid + 1, std::process::id()).is_empty(), "another user");
        let mut units = Vec::new();
        units_of(&dead, 3, &mut units);
        assert_eq!(units, [(dead.join("default"), "u-1".to_string())]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// T-407, #250: a directory of the run that another user made, or a
    /// link at its name, is refused; one of this user is made new at 0700.
    #[test]
    fn the_directories_of_a_run_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = std::env::temp_dir().join(format!("podrunner-run-dirs-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("elsewhere")).unwrap();
        let (tree, units) = run_dirs(&tmp, 7).unwrap();
        for d in [&tree, &units] {
            assert_eq!(std::fs::metadata(d).unwrap().permissions().mode() & 0o777, 0o700, "{}", d.display());
        }
        std::fs::write(tree.join("old"), "x").unwrap();
        let (tree, _) = run_dirs(&tmp, 7).unwrap();
        assert!(!tree.join("old").exists(), "a directory of this user is made new");
        std::fs::remove_dir_all(&units).unwrap();
        std::os::unix::fs::symlink(tmp.join("elsewhere"), &units).unwrap();
        let e = run_dirs(&tmp, 7).unwrap_err();
        assert!(e.contains("podrunner-run-units-7") && e.contains("not a directory of this user"), "{e}");
        assert!(std::fs::read_dir(tmp.join("elsewhere")).unwrap().next().is_none(), "nothing went through the link");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// T-323 F14: a TMPDIR that is not there is named.
    #[test]
    fn a_missing_tmpdir_is_named() {
        assert!(tmp_ok(&std::env::temp_dir()).is_ok());
        let e = tmp_ok(Path::new("/nonexistent/podrunner-tmp")).unwrap_err();
        assert!(e.starts_with("TMPDIR /nonexistent/podrunner-tmp: ") && e.contains("set TMPDIR"), "{e}");
    }
}
