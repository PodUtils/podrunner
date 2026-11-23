//! T-607 with a real second pid namespace: the test binary runs itself
//! under `unshare -rp --fork --mount-proc`, as pid 1 of a new namespace
//! with no root. A host that refuses a user namespace skips with a line.

use super::*;

const CHILD: &str = "PODRUNNER_T607_CHILD";

/// The part of the child: it writes its run id to `ID` in the directory of
/// the parent, and fails when it finds a directory of the live parent as
/// dead. It runs only under the parent below.
#[test]
#[ignore = "the child of two_namespaces_of_pid_1_do_not_meet"]
fn namespace_child() {
    let Some(tmp) = std::env::var_os(CHILD).map(PathBuf::from) else { return };
    // SAFETY: getuid has no failure.
    let uid = unsafe { libc::getuid() };
    assert_eq!(here().map(|h| h.pid), Some(1), "the child is not pid 1 of its own /proc");
    let found = dead_runs(&tmp, uid, here());
    let id = run_id();
    std::fs::write(tmp.join(format!("ID-{id}")), "").unwrap();
    assert!(found.is_empty(), "a live run of another pid namespace counts as dead: {found:?}");
}

/// Run the child in a new pid namespace; `None` when the host refuses it.
fn in_new_namespace(tmp: &Path) -> Option<std::process::ExitStatus> {
    let exe = std::env::current_exe().unwrap();
    let probe = std::process::Command::new("unshare").args(["-rp", "--fork", "--mount-proc", "true"]).output();
    if !probe.as_ref().is_ok_and(|o| o.status.success()) {
        return None;
    }
    let out = std::process::Command::new("unshare")
        .args(["-rp", "--fork", "--mount-proc"])
        .arg(exe)
        .args(["--exact", "run_local::stale::ns_tests::namespace_child", "--ignored", "--test-threads=1", "-q"])
        .env(CHILD, tmp)
        .output()
        .unwrap();
    if !out.status.success() {
        eprintln!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    }
    Some(out.status)
}

/// Two runs as pid 1 of two pid namespaces share one TMPDIR: they get two
/// ids, and neither counts the directories of this live run as dead.
#[test]
fn two_namespaces_of_pid_1_do_not_meet() {
    let tmp = std::env::temp_dir().join(format!("podrunner-stale-ns-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let (tree, units) = run_dirs(&tmp, &run_id()).unwrap();
    let Some(first) = in_new_namespace(&tmp) else {
        let _ = std::fs::remove_dir_all(&tmp);
        eprintln!("skip: this host refuses a pid namespace with no root (unshare -rp)");
        return;
    };
    let second = in_new_namespace(&tmp).unwrap();
    let mut ids: Vec<String> = std::fs::read_dir(&tmp)
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_prefix("ID-").map(str::to_string))
        .collect();
    ids.sort();
    let live = tree.is_dir() && units.is_dir();
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(first.success() && second.success(), "a child found the live run as dead");
    assert!(live, "a child removed the directories of a live run");
    assert_eq!(ids.len(), 2, "two runs of pid 1 in two namespaces meet on one id: {ids:?}");
    assert!(ids.iter().all(|id| owner(id).is_some_and(|h| h.pid == 1)), "{ids:?}");
}
