//! T-648: the cache of a job is the one host directory of an instance.

use std::fs;
use std::path::{Path, PathBuf};

use podrunner_unit::{StartSpec, UnitCode, UnitDriver};

use crate::cache::{CACHE_MOUNT, RO_CHECK};

use super::{driver, spec};

/// The argv of `podvm run -d` for a start with `mounts`, or the error.
fn start_with(name: &str, mounts: Vec<(PathBuf, String)>) -> (Result<String, UnitCode>, PathBuf) {
    let (u, d) = driver(name, "exit 0");
    let s = StartSpec { mounts, ..spec("u-1") };
    let file = d.join("store").join(format!("{}.argv", u.name(&s.unit)));
    let r = u.start(&s).map_err(|e| e.code).map(|()| {
        let argv = fs::read_to_string(&file).unwrap();
        assert!(u.destroy(&s.unit).unwrap().is_clean());
        argv
    });
    if r.is_err() {
        assert!(!file.exists(), "a refused start makes no instance");
        assert!(!u.unit_dir(&s.unit).exists(), "a refused start leaves no unit directory");
    }
    (r, d)
}

#[test]
fn the_cache_is_a_disk_device_of_its_mode_and_a_ro_cache_is_checked() {
    let cache = Path::new("/var/cache/podrunner/0123/data");
    for (target, ro) in [("/podrunner-cache", false), ("/podrunner-cache:ro", true)] {
        let (argv, d) = start_with("cache", vec![(cache.into(), target.into())]);
        let argv = argv.unwrap();
        let words: Vec<&str> = argv.lines().collect();
        let want = format!("{}:{target}", cache.display());
        let at = words.iter().position(|w| *w == want).unwrap_or_else(|| panic!("{want}: {argv}"));
        assert_eq!(words[at - 1], "-v");
        assert_eq!(argv.contains(RO_CHECK), ro, "the script checks a ro cache: {argv}");
        let _ = fs::remove_dir_all(&d);
    }
    let (argv, d) = start_with("no-cache", Vec::new());
    assert!(!argv.unwrap().contains(CACHE_MOUNT));
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn the_check_takes_only_a_read_only_mount_of_the_cache() {
    assert!(RO_CHECK.contains(&format!("\"{CACHE_MOUNT}\"")));
    let d = std::env::temp_dir().join(format!("podrunner-incus-ro-check-{}", std::process::id()));
    fs::create_dir_all(&d).unwrap();
    let f = d.join("mounts");
    let check = RO_CHECK.replace("/proc/mounts", &f.display().to_string());
    let ok = |table: &str| {
        fs::write(&f, table).unwrap();
        let out = std::process::Command::new("sh").args(["-c", &format!("set -eu\n{check}echo payload")]).output();
        String::from_utf8_lossy(&out.unwrap().stdout).trim() == "payload"
    };
    let ro = "/dev/vda1 /podrunner-cache ext4 ro,relatime 0 0\n";
    let rw = "/dev/vda1 /podrunner-cache ext4 rw,relatime 0 0\n";
    assert!(ok(ro));
    assert!(!ok(rw), "a writable cache");
    assert!(!ok("/dev/vda1 / ext4 rw 0 0\n"), "no mount of the cache");
    assert!(!ok(&format!("{ro}{rw}")), "a writable mount over it");
    assert!(ok(&format!("{rw}{ro}")), "a read-only mount over it");
    assert!(!ok("/dev/vda1 /podrunner-cache2 ext4 ro 0 0\n"), "another path");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn each_other_host_directory_is_refused_before_the_start() {
    let cache = || PathBuf::from("/c/data");
    for mounts in [
        vec![(cache(), "/data".to_string())],
        vec![(cache(), "/data:ro".to_string())],
        vec![(cache(), "/podrunner-cache2".to_string())],
        vec![(cache(), "/podrunner-cache/x".to_string())],
        vec![(cache(), "/podrunner-cache:rw".to_string())],
        vec![(cache(), "/podrunner-work".to_string())],
        vec![(cache(), "/podrunner-cache".to_string()), (cache(), "/podrunner-cache:ro".to_string())],
        vec![("c/data".into(), "/podrunner-cache".to_string())],
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
    assert_eq!(u.probe().unwrap().mount_modes, ["ro"], "no rw: the host cannot remove a file of the map");
    let _ = fs::remove_dir_all(&d);
}
