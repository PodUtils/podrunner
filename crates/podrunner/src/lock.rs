//! The lock of a state directory (GitHub #54): one `serve` for each state
//! directory. A second `serve` would replay the journal and destroy the
//! live units of the first. The lock is a `flock` on `STATE/serve.lock`:
//! the kernel frees it when the holder dies, also by SIGKILL. The file is
//! never removed: a removal lets a waiter that holds the old inode and a
//! starter that creates a new one both hold "the" lock.

use std::fs::DirBuilder;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use pod_core::failure::Class;
use pod_core::lock::{FileLock, Mode};

use crate::serve::{Failure, fail};

/// The lock file of a state directory.
pub fn path_of(state: &Path) -> PathBuf {
    state.join("serve.lock")
}

/// A held lock. Dropping it frees the lock. `pod_core::lock` opens the file
/// at mode 0600, with no link at its name, and close-on-exec: a unit that
/// serve starts does not keep the lock held after serve ends.
#[derive(Debug)]
pub struct StateLock(#[allow(dead_code)] FileLock);

impl StateLock {
    /// Take the lock of `state`, or fail with `Unavailable` (exit 69). Each
    /// error other than "held" also refuses: a destroyed job costs more
    /// than a refused start.
    pub fn take(state: &Path) -> Result<StateLock, Failure> {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(state)
            .map_err(|e| fail(Class::Unavailable, format!("the state directory: {}", e)))?;
        match FileLock::try_lock(&path_of(state), Mode::Exclusive) {
            Ok(Some(l)) => Ok(StateLock(l)),
            Ok(None) => Err(fail(Class::Unavailable, "another podrunner serve holds the state directory")),
            Err(e) => Err(fail(Class::Unavailable, format!("the lock of the state directory: {}", e))),
        }
    }
}

/// Whether no `serve` holds the lock of `state` now. A directory with no
/// lock file has no holder. The probe opens the file read-only, with no
/// create and no link at its name, so it makes no file and needs no write.
pub fn is_free(state: &Path) -> std::io::Result<bool> {
    // The probe holds the lock for an instant; the drop frees it.
    match FileLock::try_lock_existing(&path_of(state), Mode::Exclusive) {
        Ok(held) => Ok(held.is_some()),
        Err(e) if e.class == Class::NotFound => Ok(true),
        Err(e) => Err(std::io::Error::other(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_holder_is_refused_and_the_file_stays() {
        let dir = std::env::temp_dir().join(format!("podrunner-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(is_free(&dir).unwrap());
        let first = StateLock::take(&dir).unwrap();
        let second = StateLock::take(&dir).unwrap_err();
        assert_eq!(second.class, Class::Unavailable);
        assert!(second.detail.contains("another podrunner serve"), "{}", second.detail);
        assert!(!is_free(&dir).unwrap());
        drop(first);
        assert!(is_free(&dir).unwrap());
        // The file is never removed (no unlink race), and a new holder takes it.
        assert!(path_of(&dir).exists());
        let _again = StateLock::take(&dir).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(path_of(&dir)).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-687: the lock holds while its value lives, also while a child
    /// runs that serve started, and it frees at the drop with the child
    /// still alive (`FileLock` unlocks at its drop).
    #[test]
    fn the_lock_follows_its_value_not_a_child() {
        let dir = std::env::temp_dir().join(format!("podrunner-lock-child-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let held = StateLock::take(&dir).unwrap();
        let mut child = std::process::Command::new("sleep").arg("300").spawn().unwrap();
        assert!(!is_free(&dir).unwrap(), "the lock holds while its value lives");
        drop(held);
        assert!(is_free(&dir).unwrap(), "a live child does not keep the lock after the drop");
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_symlink_at_the_lock_path_is_refused() {
        let dir = std::env::temp_dir().join(format!("podrunner-lock-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::os::unix::fs::symlink("/dev/null", path_of(&dir)).unwrap();
        assert_eq!(StateLock::take(&dir).unwrap_err().class, Class::Unavailable);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
