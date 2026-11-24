//! The key of a pid namespace in the name of a run (T-615): the machine id
//! and the inode, or the boot id and the inode when there is no machine id.

use super::*;
use std::ffi::OsString;

/// Made-up machine ids and boot ids of two hosts.
const M1: &str = "0f1e2d3c4b5a69788796a5b4c3d2e1f0\n";
const M2: &str = "1f1e2d3c4b5a69788796a5b4c3d2e1f0\n";
const B1: &str = "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0\n";
const B2: &str = "1f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0\n";
/// The inode of the initial pid namespace: the same on each boot.
const INIT: u64 = 4026531836;
/// A pid that no process has: above the maximum of the kernel.
const DEAD: u32 = 4194305;

/// The directories of dead runs in `tmp` for the process `me`, after one
/// run of the key `ns` left a directory with a unit.
fn found(tag: &str, ns: Option<u64>, me: Option<u64>) -> (Vec<OsString>, OsString) {
    let tmp = crate::scratch::Scratch::new(tag);
    let ns = ns.expect("the key of the dead run");
    let dir = tmp.join(format!("podrunner-run-units-{}", name(Some(Here { ns, pid: DEAD }), 0)));
    std::fs::create_dir_all(dir.join("default/units/u-1")).unwrap();
    // SAFETY: getuid has no failure.
    let uid = unsafe { libc::getuid() };
    let me = me.map(|ns| Here { ns, pid: std::process::id() });
    let got = dead_runs(&tmp, uid, me);
    let names = got.iter().filter_map(|p| p.file_name()).map(OsString::from).collect();
    (names, dir.file_name().unwrap().into())
}

/// T-615: a run of an earlier boot of this machine is dead: one machine id,
/// the inode of the initial pid namespace, another boot id, a dead pid.
#[test]
fn a_run_of_an_earlier_boot_is_dead() {
    let (before, now) = (ns_key(INIT, Some(M1), Some(B1)), ns_key(INIT, Some(M1), Some(B2)));
    assert_eq!(before, now, "a reboot changes the key");
    let (got, dir) = found("stale-reboot", before, now);
    assert_eq!(got, [dir], "a run of an earlier boot of this machine is not found dead");
}

/// T-615: a run of another machine (another machine id) that shares the
/// TMPDIR is not dead, also with one inode, one boot id and a dead pid.
#[test]
fn a_run_of_another_machine_is_not_dead() {
    for boot in [B1, B2] {
        let (other, me) = (ns_key(INIT, Some(M2), Some(boot)), ns_key(INIT, Some(M1), Some(B1)));
        assert_ne!(other, me, "two machines give one key");
        let (got, _) = found("stale-machine", other, me);
        assert!(got.is_empty(), "a run of another machine counts as dead: {got:?}");
    }
}

/// T-615: with no machine id (no file, empty, or not 32 hex digits), the
/// boot id separates two hosts; with neither, there is no key.
#[test]
fn with_no_machine_id_the_boot_id_separates_two_hosts() {
    for none in [None, Some(""), Some(" \n"), Some("uninitialized\n"), Some("0f1e2d3c")] {
        let (a, b) = (ns_key(INIT, none, Some(B1)), ns_key(INIT, none, Some(B2)));
        assert!(a.is_some() && a != b, "{none:?}: one key for two boots");
        assert_eq!(a, ns_key(INIT, None, Some(B1)), "{none:?}");
        let (got, _) = found("stale-boot", b, a);
        assert!(got.is_empty(), "{none:?}: a run of another boot counts as dead: {got:?}");
        assert_eq!(ns_key(INIT, none, None), None, "{none:?}");
        assert_eq!(ns_key(INIT, none, Some(" \n")), None, "{none:?}");
    }
    assert_ne!(ns_key(INIT, Some(M1), Some(B1)), ns_key(INIT + 1, Some(M1), Some(B1)), "one key for two inodes");
    let m = M1.trim();
    assert_ne!(ns_key(INIT, Some(m), None), ns_key(INIT, None, Some(m)), "a machine id and a boot id meet");
    assert_eq!(ns_key(INIT, Some(m), None), ns_key(INIT, Some(M1), Some(B2)), "the trim");
}
