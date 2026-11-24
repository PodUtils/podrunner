use super::write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

/// The directory of one test; it goes at the drop, also after a panic.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let p = std::env::temp_dir().join(format!("podrunner-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Scratch(p)
    }
}

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // A directory at mode 0555 inside: make it writable first.
        let _ = std::fs::set_permissions(self.0.join("ro"), std::fs::Permissions::from_mode(0o755));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Write and start `n` programs with `put` while three threads fork; the
/// count of the starts that fail with ETXTBSY.
fn busy_starts(dir: &Path, n: usize, put: fn(&Path, &[u8]) -> std::io::Result<()>) -> usize {
    let stop = AtomicBool::new(false);
    std::thread::scope(|s| {
        for _ in 0..3 {
            s.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    let _ = Command::new("true").status();
                }
            });
        }
        let mut busy = 0;
        for i in 0..n {
            let p = dir.join(format!("p{i}"));
            put(&p, b"#!/bin/sh\nexit 0\n").unwrap();
            match Command::new(&p).status() {
                Ok(st) => assert!(st.success(), "{}: {st}", p.display()),
                Err(e) if e.raw_os_error() == Some(libc::ETXTBSY) => busy += 1,
                Err(e) => panic!("{}: {e}", p.display()),
            }
        }
        stop.store(true, Ordering::Relaxed);
        busy
    })
}

/// T-646: a program written while other threads fork starts each time.
/// The test fails with `std::fs::write` and a chmod, and with a write, a
/// sync and a rename into place in this process.
#[test]
fn a_program_written_while_threads_fork_starts_each_time() {
    let dir = Scratch::new("program-file");
    let busy = busy_starts(&dir, 1000, |p, t| write(p, t));
    assert_eq!(busy, 0, "starts that failed with ETXTBSY");
}

#[test]
fn a_program_replaces_the_old_file_with_the_mode_0755() {
    let dir = Scratch::new("program-file-mode");
    let p = dir.join("prog");
    std::fs::write(&p, "old, and longer than the new text").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
    write(&p, "#!/bin/sh\necho new\n").unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "#!/bin/sh\necho new\n");
    assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o7777, 0o755);
    let out = Command::new(&p).output().unwrap();
    assert_eq!(out.stdout, b"new\n");
    // A link at the path is replaced, not followed.
    let other = dir.join("other");
    std::fs::write(&other, "of the user").unwrap();
    std::fs::remove_file(&p).unwrap();
    std::os::unix::fs::symlink(&other, &p).unwrap();
    write(&p, "#!/bin/sh\n").unwrap();
    assert_eq!(std::fs::read_to_string(&other).unwrap(), "of the user");
    assert!(std::fs::symlink_metadata(&p).unwrap().file_type().is_file());
    std::fs::remove_file(&other).unwrap();
    // An empty program, and no temp file is left.
    write(&p, "").unwrap();
    assert_eq!(std::fs::metadata(&p).unwrap().len(), 0);
    let left: Vec<_> = std::fs::read_dir(&*dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(left, [std::ffi::OsString::from("prog")]);
}

#[test]
fn a_path_that_cannot_hold_a_program_is_refused_and_leaves_nothing() {
    let dir = Scratch::new("program-file-refused");
    // No directory.
    let e = write(dir.join("none/prog"), "x").unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::NotFound, "{e}");
    // A directory in place of the file: the rename fails.
    std::fs::create_dir(dir.join("d")).unwrap();
    assert!(write(dir.join("d"), "x").is_err());
    // No file name.
    assert_eq!(write(Path::new("/"), "x").unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
    // A directory that the user cannot write (not as root: root writes).
    // SAFETY: geteuid(2) has no precondition.
    if unsafe { libc::geteuid() } != 0 {
        let ro = dir.join("ro");
        std::fs::create_dir(&ro).unwrap();
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
        let e = write(ro.join("prog"), "x").unwrap_err();
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied, "{e}");
    }
    let mut left: Vec<_> = std::fs::read_dir(&*dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    left.sort();
    assert!(left.iter().all(|n| n == "d" || n == "ro"), "{left:?}");
    assert!(std::fs::read_dir(dir.join("d")).unwrap().next().is_none());
}
