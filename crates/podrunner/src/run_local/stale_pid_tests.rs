//! T-636, T-715: the files of the units of a dead run, which have the key
//! of its process in their names, go with the run; a zombie run is dead; the helper of a
//! third signal ends the runs that died.

use super::*;

/// A pid above the limit of the kernel: no process has it.
const DEAD: u32 = 4_194_305;

fn uid() -> u32 {
    // SAFETY: getuid has no failure.
    unsafe { libc::getuid() }
}

/// The key of the run process `pid` of the pid namespace `ns` (T-715).
fn key(ns: u64, pid: u32) -> String {
    key_of(Some(Here { ns, pid }))
}

/// The socket directory, its logs, the socket of the broker and the export
/// directory of a dead run go; the same files of another pid, of a run of
/// another pid namespace with the same pid (T-715), of an older release
/// (only a pid in the name), a plain file at the name of a socket and a
/// link stay.
#[test]
fn the_unit_files_of_a_dead_run_go_with_it() {
    let tmp = std::env::temp_dir().join(format!("podrunner-stale-pid-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let me = Here { ns: 77, pid: std::process::id() };
    let (k, other, shared) = (key(77, DEAD), key(77, DEAD + 1), key(78, DEAD));
    for d in [
        format!("podrunner-run-units-77-{DEAD}-0/default/units/u-1"),
        format!("podrunner-local-unit-{k}-u-1"),
        format!("podrunner-local-unit-{k}-u-1.logs/ca"),
        format!("podrunner-run-export-{k}-u-1/work"),
        format!("podrunner-local-unit-{other}-u-1"),
        format!("podrunner-run-tree-78-{DEAD}-0"),
        format!("podrunner-local-unit-{shared}-u-1"),
        format!("podrunner-local-unit-{DEAD}-u-1"),
    ] {
        std::fs::create_dir_all(tmp.join(d)).unwrap();
    }
    std::fs::write(tmp.join(format!("podrunner-local-unit-{k}-u-1/ctl.sock")), "").unwrap();
    let sock = tmp.join(format!("podrunner-local-broker-{k}-1.sock"));
    drop(std::os::unix::net::UnixListener::bind(&sock).unwrap());
    std::fs::write(tmp.join(format!("podrunner-local-broker-{k}-2.sock")), "").unwrap();
    std::os::unix::fs::symlink(&tmp, tmp.join(format!("podrunner-local-unit-{k}-u-9"))).unwrap();
    let found = |uid| {
        let mut f = of_keys(&tmp, uid, &dead_runs(&tmp, uid, Some(me)));
        f.sort();
        f
    };
    let (got, none) = (found(uid()), found(uid() + 1));
    end_dead_runs_in(&tmp, uid(), Some(me));
    let mut left: Vec<String> =
        std::fs::read_dir(&tmp).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    let _ = std::fs::remove_dir_all(&tmp);
    let mut want = [
        format!("podrunner-local-broker-{k}-1.sock"),
        format!("podrunner-local-unit-{k}-u-1"),
        format!("podrunner-local-unit-{k}-u-1.logs"),
        format!("podrunner-run-export-{k}-u-1"),
    ];
    want.sort();
    assert_eq!(got, want.iter().map(|n| tmp.join(n)).collect::<Vec<_>>());
    assert!(none.is_empty(), "the files of another user: {none:?}");
    let mut kept = [
        format!("podrunner-local-broker-{k}-2.sock"),
        format!("podrunner-local-unit-{k}-u-9"),
        format!("podrunner-local-unit-{other}-u-1"),
        format!("podrunner-local-unit-{shared}-u-1"),
        format!("podrunner-local-unit-{DEAD}-u-1"),
        format!("podrunner-run-tree-78-{DEAD}-0"),
    ];
    kept.sort();
    assert_eq!(left, kept, "the dead runs and only their files go");
}

/// A zombie is not a live run: the helper of a third signal ends the run
/// before the parent of the run collects it.
#[test]
fn a_zombie_run_is_dead() {
    assert!(live_podrunner(std::process::id()));
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let t = std::time::Instant::now();
    // The event: the state of the child is Z.
    while podrunner_unit::procs::stat(child.id()).is_some_and(|s| s.state != 'Z') {
        assert!(t.elapsed() < std::time::Duration::from_secs(30), "no zombie");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let live = live_podrunner(child.id());
    child.wait().unwrap();
    assert!(!live, "a zombie counts as live");
    assert!(!live_podrunner(DEAD));
}

/// The helper refuses other arguments with 64; with a dead pid it ends
/// each dead run of `TMPDIR` of this pid namespace.
#[test]
fn the_helper_ends_the_dead_runs() {
    use super::super::reap::helper;
    let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(helper(&a(&[])), 64);
    assert_eq!(helper(&a(&["/tmp"])), 64);
    assert_eq!(helper(&a(&["/tmp", "x"])), 64);
    assert_eq!(helper(&a(&["/tmp", "1", "2"])), 64);
    let Some(h) = here() else { return };
    let tmp = std::env::temp_dir().join(format!("podrunner-stale-helper-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let dead = tmp.join(format!("podrunner-run-units-{}-{DEAD}-0", h.ns));
    std::fs::create_dir_all(dead.join("default/units/u-1")).unwrap();
    std::fs::create_dir_all(tmp.join(format!("podrunner-local-unit-{}-u-1", key(h.ns, DEAD)))).unwrap();
    let code = helper(&a(&[tmp.to_str().unwrap(), &DEAD.to_string()]));
    let left = std::fs::read_dir(&tmp).unwrap().count();
    let _ = std::fs::remove_dir_all(&tmp);
    assert_eq!((code, left), (0, 0));
}
