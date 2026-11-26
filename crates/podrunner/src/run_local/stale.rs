//! The units of a run that died (T-312): a SIGKILL of podrunner leaves its
//! podvm units running and its directories `podrunner-run-units-ID` and
//! `podrunner-run-tree-ID` in `TMPDIR`. The next run of the same user in
//! the same pid namespace ends each process of those units (by the tag of
//! each unit, as `destroy` finds them) and removes the directories, and
//! says so. `ID` is `NS-PID-N` ([`run_id`]): two containers that share
//! `TMPDIR` can each have a run as pid 1 (T-607), and NS has the machine
//! id, so two hosts that share it do not meet (T-615).

use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

const PREFIXES: &[&str] = &["podrunner-run-units-", "podrunner-run-tree-"];
/// The probe of `TMPDIR` of a run ([`tmp_ok`]); a dead run can leave it.
const PROBE: &str = "podrunner-probe-";

/// A process as `/proc` sees it: the key of its pid namespace ([`ns_key`])
/// and its pid in that namespace.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Here {
    ns: u64,
    pid: u32,
}

/// This process, or `None` when `/proc` is not the one of its pid
/// namespace (no `/proc`, or a mount of another namespace): a pid of
/// `/proc` then names another process, so no run counts as dead. Also
/// `None` with no machine id and no boot id ([`ns_key`]): then a run of
/// another host is not known.
pub(super) fn here() -> Option<Here> {
    let pid = std::process::id();
    let seen: u32 = std::fs::read_link("/proc/self").ok()?.to_str()?.parse().ok()?;
    let inode = std::fs::metadata("/proc/self/ns/pid").ok()?.ino();
    let machine = std::fs::read_to_string("/etc/machine-id").ok();
    let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok();
    let ns = ns_key(inode, machine.as_deref(), boot.as_deref())?;
    (seen == pid).then_some(Here { ns, pid })
}

/// The key of a pid namespace: FNV-1a of 64 bits of the machine id and
/// the inode, a fixed key for each pair. An inode is unique on one kernel
/// only: two hosts that share one `TMPDIR` (NFS, a shared home) can give
/// one inode and one pid, and their machine ids differ (T-615). The
/// machine id stays over a reboot, so a run of an earlier boot of this
/// machine is found dead; a pid that a new process took only keeps it.
/// With no machine id (no file, or not 32 hex digits), the boot id; with
/// neither, `None`. Two hosts with one copied machine id are one host.
fn ns_key(inode: u64, machine: Option<&str>, boot: Option<&str>) -> Option<u64> {
    let machine = machine.map(str::trim).filter(|m| m.len() == 32 && m.bytes().all(|b| b.is_ascii_hexdigit()));
    let (tag, id) = match (machine, boot.map(str::trim).filter(|b| !b.is_empty())) {
        (Some(m), _) => (b'm', m),
        (None, Some(b)) => (b'b', b),
        (None, None) => return None,
    };
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in [tag].into_iter().chain(id.bytes()).chain([0]).chain(inode.to_le_bytes()) {
        h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    Some(h)
}

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

/// The tree directory and the units directory of the run `id` under
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

/// The name of a run of this process ([`name`]).
pub(super) fn run_id() -> String {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    name(here(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

/// The name of the run `n` of the process `here`: `NS-PID-N`. Two runs of
/// one process (the tests) get two names, two runs of one pid in two pid
/// namespaces too, and a dead run is found by its namespace and pid. With
/// no `here`, the name starts with `x` and the time in nanoseconds: it is
/// unique, and [`owner`] gives no process for it, so no run ends it.
fn name(here: Option<Here>, n: u32) -> String {
    match here {
        Some(Here { ns, pid }) => format!("{ns}-{pid}-{n}"),
        None => {
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            format!("x{:x}-{}-{n}", t.as_nanos(), std::process::id())
        }
    }
}

/// The process of the run `id` ([`name`]). A name of another form (a name
/// of an older release, `PID` or `PID-N`, which has no namespace) gives
/// none: its run can be live in another pid namespace.
fn owner(id: &str) -> Option<Here> {
    let mut parts = id.split('-').map(|s| (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())).then_some(s));
    let (ns, pid, _n) = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() {
        return None;
    }
    Some(Here { ns: ns.parse().ok()?, pid: pid.parse().ok()? })
}

/// Whether a process `pid` lives. Any process counts, not only one named
/// podrunner: a run started through a link of another name stays safe. A
/// zombie is dead: the helper of a third signal ends the run before the
/// parent of the run collects it (T-636).
fn live_podrunner(pid: u32) -> bool {
    podrunner_unit::procs::stat(pid).is_some_and(|s| !matches!(s.state, 'Z' | 'X'))
}

/// The directories of dead runs of this user in `tmp`, and the probe that
/// a dead run left. A run is dead when its namespace is the one of `me`,
/// its pid is not the one of `me`, and no process has its pid. With no
/// `me`, no run is dead.
fn dead_runs(tmp: &Path, uid: u32, me: Option<Here>) -> Vec<PathBuf> {
    let Some(me) = me else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(tmp) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let run = owner(PREFIXES.iter().chain([&PROBE]).find_map(|p| name.strip_prefix(p))?)?;
            let meta = std::fs::symlink_metadata(e.path()).ok()?;
            let dead = run.ns == me.ns && run.pid != me.pid && !live_podrunner(run.pid);
            (meta.is_dir() && meta.uid() == uid && dead).then(|| e.path())
        })
        .collect()
}

/// The files of the units of a local run that have the pid of the run in
/// their names, not its key: the socket directory of each unit and its
/// logs, the socket of each broker, and each export directory (T-636).
const OF_PID: &[&str] = &["podrunner-local-unit-", "podrunner-local-broker-", "podrunner-run-export-"];

/// The files of [`OF_PID`] of this user in `tmp` that have the pid of a
/// dead run in `dead`. A pid that a run of another pid namespace also
/// has (a directory of [`PREFIXES`] with its key) keeps its files: that
/// run can be live, and its files have the same names.
fn of_pids(tmp: &Path, uid: u32, me: Here, dead: &[PathBuf]) -> Vec<PathBuf> {
    let names: Vec<(PathBuf, String)> = match std::fs::read_dir(tmp) {
        Ok(entries) => entries.flatten().map(|e| (e.path(), e.file_name().to_string_lossy().into_owned())).collect(),
        Err(_) => return Vec::new(),
    };
    let run = |name: &str, probe: bool| {
        let probe = probe.then_some(&PROBE);
        owner(PREFIXES.iter().chain(probe).find_map(|p| name.strip_prefix(p))?)
    };
    let dead: Vec<u32> =
        dead.iter().filter_map(|d| run(&d.file_name()?.to_string_lossy(), true)).map(|h| h.pid).collect();
    let shared: Vec<u32> =
        names.iter().filter_map(|(_, n)| run(n, false)).filter(|h| h.ns != me.ns).map(|h| h.pid).collect();
    names
        .into_iter()
        .filter_map(|(path, name)| {
            let pid: u32 = OF_PID.iter().find_map(|p| name.strip_prefix(p))?.split('-').next()?.parse().ok()?;
            let meta = std::fs::symlink_metadata(&path).ok()?;
            let kind = meta.is_dir() || meta.file_type().is_socket();
            (kind && meta.uid() == uid && dead.contains(&pid) && !shared.contains(&pid)).then_some(path)
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
    end_dead_runs_in(&std::env::temp_dir(), uid, here());
}

pub(super) fn end_dead_runs_in(tmp: &Path, uid: u32, me: Option<Here>) {
    let dead = dead_runs(tmp, uid, me);
    let by_pid = me.map(|me| of_pids(tmp, uid, me, &dead)).unwrap_or_default();
    for dir in dead.into_iter().chain(by_pid) {
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
        let removed = match dir.is_dir() {
            true => podrunner_local::work::clear(&dir),
            false => std::fs::remove_file(&dir).map_err(|e| e.to_string()),
        };
        match removed {
            Ok(()) => eprintln!("podrunner: {}: the files of a run that died are gone{what}", dir.display()),
            Err(e) => eprintln!("podrunner: a run that died left {}: {e}", dir.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The process of a test: pid namespace 77 (made up), this pid.
    fn me() -> Here {
        Here { ns: 77, pid: std::process::id() }
    }

    /// #237: a dry run leaves the directories of a dead run.
    #[test]
    fn a_dry_run_leaves_a_dead_run() {
        let ns = here().map_or(77, |h| h.ns);
        let dead = std::env::temp_dir().join(format!("podrunner-run-units-{ns}-4194307-0"));
        std::fs::create_dir_all(dead.join("default/units/u-1")).unwrap();
        end_dead_runs(true);
        assert!(dead.is_dir(), "a dry run removed the files of a dead run");
        let _ = std::fs::remove_dir_all(&dead);
    }

    /// T-607: the name of a run has the pid namespace, so two runs of one
    /// pid in two namespaces get two names; each name gives its process
    /// back. A name with no process (no `/proc` of this namespace) gives
    /// none, and so does each name of another form.
    #[test]
    fn a_run_is_named_by_its_namespace_pid_and_count() {
        let (a, b) = (Here { ns: 4026531836, pid: 1 }, Here { ns: 4026532266, pid: 1 });
        assert_eq!(name(Some(a), 0), "4026531836-1-0");
        assert_ne!(name(Some(a), 0), name(Some(b), 0));
        assert_ne!(name(Some(a), 0), name(Some(a), 1));
        assert_eq!((owner(&name(Some(a), 0)), owner(&name(Some(b), 7))), (Some(a), Some(b)));
        let none = name(None, 3);
        assert!(none.starts_with('x') && none.ends_with(&format!("-{}-3", std::process::id())), "{none}");
        assert_eq!(owner(&none), None);
        for id in ["", "7", "7-0", "1-7-0-9", "1--0", "-7-0", "+1-7-0", "1-7-0 ", "1-x-0", "18446744073709551616-7-0"] {
            assert_eq!(owner(id), None, "{id:?}");
        }
        assert_eq!(owner("18446744073709551615-4294967295-0").map(|h| h.pid), Some(u32::MAX));
        assert_eq!(owner("1-4294967296-0"), None);
        if let Some(h) = here() {
            assert_eq!((owner(&run_id()), h.pid), (Some(h), std::process::id()));
        }
    }

    /// A directory of a dead run of this pid namespace is found and ended;
    /// one of a live process, of this process, of another namespace
    /// (T-607), of another form or of another name is not.
    #[test]
    fn the_directories_of_dead_runs_are_found() {
        let tmp = std::env::temp_dir().join(format!("podrunner-stale-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let pid = std::process::id();
        // A pid that no process has: above the maximum of the kernel.
        let dead = tmp.join("podrunner-run-units-77-4194305-0");
        std::fs::create_dir_all(dead.join("default/units/u-1")).unwrap();
        let other_ns = tmp.join("podrunner-run-units-78-4194305-0");
        std::fs::create_dir_all(other_ns.join("default/units/u-1")).unwrap();
        for d in [
            "podrunner-run-tree-77-4194306-1".to_string(),
            format!("podrunner-run-units-77-{pid}-0"),
            format!("podrunner-run-tree-77-{pid}-3"),
            "podrunner-run-tree-77-1-0".into(),
            "podrunner-run-tree-78-4194306-1".into(),
            "podrunner-run-tree-4194307".into(),
            "podrunner-run-tree-4194307-2".into(),
            "podrunner-run-tree-77-4194307-2-9".into(),
            "podrunner-run-tree-x1f-4194307-0".into(),
            "other-77-4194305-0".into(),
        ] {
            std::fs::create_dir_all(tmp.join(d)).unwrap();
        }
        // SAFETY: getuid has no failure.
        let uid = unsafe { libc::getuid() };
        let mut got = dead_runs(&tmp, uid, Some(me()));
        got.sort();
        assert_eq!(got, [tmp.join("podrunner-run-tree-77-4194306-1"), dead.clone()]);
        assert!(dead_runs(&tmp, uid + 1, Some(me())).is_empty(), "another user");
        assert!(dead_runs(&tmp, uid, None).is_empty(), "no /proc of this namespace");
        let mut units = Vec::new();
        units_of(&dead, 3, &mut units);
        assert_eq!(units, [(dead.join("default"), "u-1".to_string())]);
        end_dead_runs_in(&tmp, uid, Some(me()));
        let (gone, kept) = (!dead.exists(), other_ns.join("default/units/u-1").is_dir());
        let left = std::fs::read_dir(&tmp).unwrap().count();
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(gone, "the directory of a dead run stays");
        assert!(kept, "the directory of a run of another pid namespace is gone");
        assert_eq!(left, 10, "only the two dead runs go");
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
    /// it. One of this process, of another pid namespace, a file, or a
    /// link, stays.
    #[test]
    fn the_probe_of_a_dead_run_is_removed() {
        let tmp = std::env::temp_dir().join(format!("podrunner-stale-probe-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let pid = std::process::id();
        let dead = tmp.join("podrunner-probe-77-4194305-2");
        std::fs::create_dir_all(&dead).unwrap();
        std::fs::create_dir_all(tmp.join(format!("podrunner-probe-77-{pid}-4"))).unwrap();
        std::fs::create_dir_all(tmp.join("podrunner-probe-78-4194305-2")).unwrap();
        std::fs::write(tmp.join("podrunner-probe-77-4194306-1"), "x").unwrap();
        std::os::unix::fs::symlink(&dead, tmp.join("podrunner-probe-77-4194307-1")).unwrap();
        // SAFETY: getuid has no failure.
        let uid = unsafe { libc::getuid() };
        let found = dead_runs(&tmp, uid, Some(me()));
        end_dead_runs_in(&tmp, uid, Some(me()));
        let gone = !dead.exists();
        let mine = tmp.join(format!("podrunner-probe-77-{pid}-4")).is_dir();
        let other = tmp.join("podrunner-probe-78-4194305-2").is_dir();
        let _ = std::fs::remove_dir_all(&tmp);
        assert_eq!(found, [dead]);
        assert!(gone, "the probe of a dead run stays");
        assert!(mine && other, "the probe of this process or of another pid namespace is gone");
    }

    /// T-323 F14: a TMPDIR that is not there is named.
    #[test]
    fn a_missing_tmpdir_is_named() {
        assert!(tmp_ok(&std::env::temp_dir(), &run_id()).is_ok());
        let e = tmp_ok(Path::new("/nonexistent/podrunner-tmp"), "7-2").unwrap_err();
        assert!(e.starts_with("TMPDIR /nonexistent/podrunner-tmp: ") && e.contains("set TMPDIR"), "{e}");
    }
}

#[cfg(test)]
#[path = "stale_ns_tests.rs"]
mod ns_tests;

#[cfg(test)]
#[path = "stale_key_tests.rs"]
mod key_tests;

#[cfg(test)]
#[path = "stale_pid_tests.rs"]
mod pid_tests;
