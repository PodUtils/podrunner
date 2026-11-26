//! T-648: the cache of a job is the one host directory of a guest.

use std::fs;
use std::path::Path;
use std::time::Duration;

use podrunner_unit::{Exit, UnitCode, UnitDriver};

use crate::guest::{MOUNT_CHECK, mount_check};
use crate::tests::{dir, driver, spec};

/// The fake podvm writes its argv, one word for each line, and exits 0.
const KEEP_ARGV: &str = r#"printf '%s\n' "$@" > "${0%/*}/argv"; exit 0"#;

/// The argv of `podvm run` for a start with `mounts`, or the error.
fn start_with(name: &str, mounts: Vec<(std::path::PathBuf, String)>) -> (Result<String, UnitCode>, std::path::PathBuf) {
    let (u, d) = driver(name, KEEP_ARGV);
    let s = podrunner_unit::StartSpec { mounts, ..spec("u-1") };
    let r = u.start(&s).map_err(|e| e.code).map(|()| {
        assert_eq!(u.wait(&s.unit, Duration::from_secs(10)).unwrap(), Some(Exit::Code(0)));
        let argv = fs::read_to_string(d.join("argv")).unwrap();
        assert!(u.destroy(&s.unit).unwrap().is_clean());
        argv
    });
    if r.is_err() {
        assert!(!d.join("argv").exists(), "a refused start runs no podvm");
        assert!(!d.join("state/units/u-1").exists(), "a refused start leaves no unit directory");
    }
    (r, d)
}

#[test]
fn the_cache_is_a_share_of_its_mode_and_the_guest_checks_the_mode() {
    let cache = Path::new("/var/cache/podrunner/0123/data");
    for (target, ro) in [("/podrunner-cache", false), ("/podrunner-cache:ro", true)] {
        let (argv, d) = start_with("cache", vec![(cache.into(), target.into())]);
        let argv = argv.unwrap();
        let words: Vec<&str> = argv.lines().collect();
        let want = format!("{}:{target}", cache.display());
        let at = words.iter().position(|w| *w == want).unwrap_or_else(|| panic!("{want}: {argv}"));
        assert_eq!(words[at - 1], "-v");
        assert_eq!(argv.matches("/podrunner-cache").count(), 2, "the share and the check: {argv}");
        assert!(argv.contains(&mount_check(Some(ro))), "the script checks the mode: {argv}");
        let _ = fs::remove_dir_all(&d);
    }
    // With no cache, the script has the check of the table alone.
    let (argv, d) = start_with("no-cache", Vec::new());
    let argv = argv.unwrap();
    assert!(argv.contains(MOUNT_CHECK) && !argv.contains("/podrunner-cache"), "{argv}");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn the_mount_check_holds_a_ro_cache_read_only_and_refuses_a_cache_that_it_did_not_get() {
    let d = dir("cache-check");
    let f = d.join("mounts");
    let ok = |check: &str, table: &str| {
        fs::write(&f, table).unwrap();
        std::process::Command::new("awk").arg(check).arg(&f).status().unwrap().success()
    };
    let work = "w /podrunner-work 9p rw,relatime,trans=virtio 0 0\n";
    let ro = format!("{work}c /podrunner-cache 9p ro,relatime,trans=virtio 0 0\n");
    let rw = format!("{work}c /podrunner-cache 9p rw,relatime,trans=virtio 0 0\n");
    assert!(ok(&mount_check(Some(true)), &ro), "a ro cache, read-only");
    assert!(!ok(&mount_check(Some(true)), &rw), "a ro cache, writable");
    assert!(ok(&mount_check(Some(false)), &rw), "a rw cache");
    assert!(ok(&mount_check(Some(false)), &ro), "a rw cache that the host gave read-only");
    assert!(!ok(&mount_check(None), &ro), "no cache in the start");
    assert!(ok(&mount_check(None), work));
    // The cache takes its own path only.
    let other = format!("{work}c /podrunner-cache2 9p ro,trans=virtio 0 0\n");
    assert!(!ok(&mount_check(Some(true)), &other));
    assert!(!ok(&mount_check(Some(false)), &format!("{rw}h /mnt/home 9p rw 0 0\n")), "a host path outside the table");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn each_other_host_directory_is_refused_before_the_start() {
    let cache = || std::path::PathBuf::from("/c/data");
    for mounts in [
        vec![(cache(), "/data".to_string())],
        vec![(cache(), "/data:ro".to_string())],
        vec![(cache(), "/podrunner-cache2".to_string())],
        vec![(cache(), "/podrunner-cache/x".to_string())],
        vec![(cache(), "/podrunner-cache:rw".to_string())],
        vec![(cache(), "/podrunner-work".to_string())],
        vec![(cache(), "/podrunner-cache".to_string()), (cache(), "/podrunner-cache:ro".to_string())],
        vec![("c/data".into(), "/podrunner-cache".to_string())],
        vec![("/c,readonly=off/data".into(), "/podrunner-cache:ro".to_string())],
        vec![("/c:x/data".into(), "/podrunner-cache".to_string())],
    ] {
        let (r, d) = start_with("refuse", mounts.clone());
        assert_eq!(r.unwrap_err(), UnitCode::UnitStartFailed, "{mounts:?}");
        let _ = fs::remove_dir_all(&d);
    }
}

#[test]
fn the_kind_mounts_the_cache_and_lists_ro_only() {
    let (u, d) = driver("cache-probe", "exit 0");
    assert!(u.host_mounts());
    assert_eq!(u.probe().unwrap().mount_modes, ["ro"], "no rw: a file of the guest user is the host user's");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_state_directory_with_a_comma_is_refused() {
    let (u, d) = driver("comma", KEEP_ARGV);
    let u = super::MicrovmUnit::new(u.podvm.clone(), &d.join("a,b"), &d.join("images"));
    u.prepare(crate::tests::IMAGE, &u.platform.clone(), false).unwrap();
    assert_eq!(u.start(&spec("u-1")).unwrap_err().code, UnitCode::UnitStartFailed);
    assert!(!d.join("argv").exists());
    let _ = fs::remove_dir_all(&d);
}
