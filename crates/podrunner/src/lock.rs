//! The lock of a state directory (GitHub #54): one `serve` for each state
//! directory. A second `serve` would replay the journal and destroy the
//! live units of the first. The lock is a `flock` on `STATE/serve.lock`:
//! the kernel frees it when the holder dies, also by SIGKILL. The file is
//! never removed: a removal lets a waiter that holds the old inode and a
//! starter that creates a new one both hold "the" lock.

use std::fs::{DirBuilder, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use pod_core::failure::Class;

use crate::serve::{Failure, fail};

/// The lock file of a state directory.
pub fn path_of(state: &Path) -> PathBuf {
    state.join("serve.lock")
}

/// A held lock. Dropping it frees the lock.
#[derive(Debug)]
pub struct StateLock(#[allow(dead_code)] File);

fn open(state: &Path) -> std::io::Result<File> {
    // O_CLOEXEC is the default of std: a unit that serve starts does not
    // keep the lock held after serve ends.
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path_of(state))
}

/// Try the lock on `f`: `Ok(true)` held now, `Ok(false)` another holds it.
fn try_flock(f: &File) -> std::io::Result<bool> {
    // SAFETY: flock(2) on a descriptor that `f` owns.
    let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        return Ok(true);
    }
    let e = std::io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::EWOULDBLOCK) { Ok(false) } else { Err(e) }
}

impl StateLock {
    /// Take the lock of `state`, or fail with `Unavailable` (exit 69). Each
    /// error other than "held" also refuses: a destroyed job costs more
    /// than a refused start.
    pub fn take(state: &Path) -> Result<StateLock, Failure> {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(state)
            .map_err(|e| fail(Class::Unavailable, format!("the state directory: {}", e.kind())))?;
        let f = open(state)
            .map_err(|e| fail(Class::Unavailable, format!("the lock of the state directory: {}", e.kind())))?;
        match try_flock(&f) {
            Ok(true) => Ok(StateLock(f)),
            Ok(false) => Err(fail(Class::Unavailable, "another podrunner serve holds the state directory")),
            Err(e) => Err(fail(Class::Unavailable, format!("the lock of the state directory: {}", e.kind()))),
        }
    }
}

/// Whether no `serve` holds the lock of `state` now. A directory with no
/// lock file has no holder.
pub fn is_free(state: &Path) -> std::io::Result<bool> {
    let f = match OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(path_of(state)) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(e) => return Err(e),
    };
    // The probe holds the lock for an instant; dropping `f` frees it.
    try_flock(&f)
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
