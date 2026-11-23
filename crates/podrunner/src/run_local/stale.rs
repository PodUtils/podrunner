//! The units of a run that died (T-312): a SIGKILL of podrunner leaves its
//! podvm units running and its directories `podrunner-run-units-PID` and
//! `podrunner-run-tree-PID` in `TMPDIR`. The next run of the same user
//! ends each process of those units (by the tag of each unit, as `destroy`
//! finds them) and removes the directories, and says so.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

const PREFIXES: &[&str] = &["podrunner-run-units-", "podrunner-run-tree-"];
/// The probe of `TMPDIR` of a run ([`tmp_ok`]); a dead run can leave it.
const PROBE: &str = "podrunner-probe-";

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
/// directory, or each later step fails with a path only (T-323 F14). The
/// probe has the name of the run `id`, so two runs of one process do not
/// meet. A killed run can leave its probe, and a later run can get the same
/// pid and count (pid reuse in a container): a directory of this user at
/// that name, not a link, is removed and the probe is made once more.
fn tmp_ok(tmp: &Path, id: &str) -> Result<(), String> {
    let probe = tmp.join(format!("{PROBE}{id}"));
    let fault =
        |e: std::io::Error| format!("TMPDIR {}: {e}: set TMPDIR to a directory that you can write", tmp.display());
    match std::fs::create_dir(&probe) {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && own_dir(&probe) => {
            std::fs::remove_dir(&probe).map_err(fault)?;
            std::fs::create_dir(&probe).map_err(fault)?
        }
        r => r.map_err(fault)?,
    }
    let _ = std::fs::remove_dir(&probe);
    Ok(())
}

/// Whether `path` is a directory of this user, not a link.
fn own_dir(path: &Path) -> bool {
    // SAFETY: getuid has no failure.
    let uid = unsafe { libc::getuid() };
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && m.uid() == uid)
}

/// The tree directory and the units directory of the run `pid` under
/// `tmp`, each new at 0700 (T-407, #250): a name that another user made
/// first, or a link, is refused, so no write of the run follows it.
/// A `tmp` that cannot take a directory is named first (T-323 F14). `id`
/// is [`run_id`].
pub(super) fn run_dirs(tmp: &Path, id: &str) -> Result<(PathBuf, PathBuf), String> {
    tmp_ok(tmp, id)?;
    let (tree, units) = (tmp.join(format!("{}{id}", PREFIXES[1])), tmp.join(format!("{}{id}", PREFIXES[0])));
    podrunner_local::work::private_dir(&tree)?;
    podrunner_local::work::private_dir(&units)?;
    Ok((tree, units))
}

/// The name of a run: the pid and a count of the runs of this process
/// (`PID-N`): two runs of one process (the tests) get two directories, and
/// a dead run is found by its pid.
pub(super) fn run_id() -> String {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    format!("{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

/// Whether a process `pid` lives. Any process counts, not only one named
/// podrunner: a run started through a link of another name stays safe.
fn live_podrunner(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// The directories of dead runs of this user in `tmp`, and the probe that
/// a dead run left.
pub(super) fn dead_runs(tmp: &Path, uid: u32, me: u32) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(tmp) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let id = PREFIXES.iter().chain([&PROBE]).find_map(|p| name.strip_prefix(p))?;
            let pid: u32 = id.split_once('-').map_or(id, |(p, _)| p).parse().ok()?;
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
    end_dead_runs_in(&std::env::temp_dir(), uid, std::process::id());
}

fn end_dead_runs_in(tmp: &Path, uid: u32, me: u32) {
    for dir in dead_runs(tmp, uid, me) {
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
        // The names `PID-N` of a run: a dead pid is found, this one is not.
        std::fs::create_dir_all(tmp.join("podrunner-run-tree-4194307-2")).unwrap();
        std::fs::create_dir_all(tmp.join(format!("podrunner-run-tree-{}-3", std::process::id()))).unwrap();
        // SAFETY: getuid has no failure.
        let uid = unsafe { libc::getuid() };
        let mut got = dead_runs(&tmp, uid, std::process::id());
        got.sort();
        let names = [tmp.join("podrunner-run-tree-4194306"), tmp.join("podrunner-run-tree-4194307-2"), dead.clone()];
        assert_eq!(got, names);
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
        let (tree, units) = run_dirs(&tmp, "7-0").unwrap();
        for d in [&tree, &units] {
            assert_eq!(std::fs::metadata(d).unwrap().permissions().mode() & 0o777, 0o700, "{}", d.display());
        }
        std::fs::write(tree.join("old"), "x").unwrap();
        let (tree, _) = run_dirs(&tmp, "7-0").unwrap();
        assert!(!tree.join("old").exists(), "a directory of this user is made new");
        std::fs::remove_dir_all(&units).unwrap();
        std::os::unix::fs::symlink(tmp.join("elsewhere"), &units).unwrap();
        let e = run_dirs(&tmp, "7-0").unwrap_err();
        assert!(e.contains("podrunner-run-units-7-0") && e.contains("not a directory of this user"), "{e}");
        assert!(std::fs::read_dir(tmp.join("elsewhere")).unwrap().next().is_none(), "nothing went through the link");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Two runs of one process (the tests run in parallel), or a probe
    /// that a killed run of the same pid left, do not make the check of
    /// TMPDIR fail: the probe has the name of the run.
    #[test]
    fn a_probe_of_another_run_of_this_pid_is_no_fault() {
        let tmp = std::env::temp_dir().join(format!("podrunner-probe-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join(format!("podrunner-probe-{}", std::process::id()))).unwrap();
        let got = run_dirs(&tmp, "7-1");
        let _ = std::fs::remove_dir_all(&tmp);
        got.unwrap();
    }

    /// A killed run leaves its probe; a later run whose pid and count are
    /// the same (pid reuse in a container) removes it and goes on. A link
    /// at the name of the probe stays a fault, and nothing goes through it.
    #[test]
    fn a_probe_that_a_killed_run_of_this_id_left_is_removed() {
        let tmp = std::env::temp_dir().join(format!("podrunner-probe-left-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("podrunner-probe-7-3")).unwrap();
        std::fs::create_dir_all(tmp.join("elsewhere")).unwrap();
        std::os::unix::fs::symlink(tmp.join("elsewhere"), tmp.join("podrunner-probe-7-4")).unwrap();
        let got = run_dirs(&tmp, "7-3");
        let left = tmp.join("podrunner-probe-7-3").exists();
        let link = tmp_ok(&tmp, "7-4");
        let elsewhere = tmp.join("elsewhere").is_dir();
        let _ = std::fs::remove_dir_all(&tmp);
        got.unwrap();
        assert!(!left, "the probe of the killed run stays");
        assert!(link.unwrap_err().contains("set TMPDIR"), "a link at the name of the probe is taken");
        assert!(elsewhere, "the target of the link is gone");
    }

    /// The probe of a dead run is a file of that run: the next run removes
    /// it. One of this process, a file, or a link, stays.
    #[test]
    fn the_probe_of_a_dead_run_is_removed() {
        let tmp = std::env::temp_dir().join(format!("podrunner-stale-probe-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let me = std::process::id();
        let dead = tmp.join("podrunner-probe-4194305-2");
        std::fs::create_dir_all(&dead).unwrap();
        std::fs::create_dir_all(tmp.join(format!("podrunner-probe-{me}-4"))).unwrap();
        std::fs::write(tmp.join("podrunner-probe-4194306-1"), "x").unwrap();
        std::os::unix::fs::symlink(&dead, tmp.join("podrunner-probe-4194307-1")).unwrap();
        // SAFETY: getuid has no failure.
        let uid = unsafe { libc::getuid() };
        let found = dead_runs(&tmp, uid, me);
        end_dead_runs_in(&tmp, uid, me);
        let (gone, mine) = (!dead.exists(), tmp.join(format!("podrunner-probe-{me}-4")).is_dir());
        let _ = std::fs::remove_dir_all(&tmp);
        assert_eq!(found, [dead]);
        assert!(gone, "the probe of a dead run stays");
        assert!(mine, "the probe of this process is gone");
    }

    /// T-323 F14: a TMPDIR that is not there is named.
    #[test]
    fn a_missing_tmpdir_is_named() {
        assert!(tmp_ok(&std::env::temp_dir(), &run_id()).is_ok());
        let e = tmp_ok(Path::new("/nonexistent/podrunner-tmp"), "7-2").unwrap_err();
        assert!(e.starts_with("TMPDIR /nonexistent/podrunner-tmp: ") && e.contains("set TMPDIR"), "{e}");
    }
}
