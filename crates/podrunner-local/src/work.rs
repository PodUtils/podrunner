//! The work directory of a runner directory (T-146). Each job starts in a
//! fresh `_work`. A step can leave a directory with no write bit, which
//! `remove_dir_all` cannot empty: the next job would then run in the stale
//! tree with no message. So each directory gets its write bit back before
//! the removal, and a failure names the path.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Give each directory under `dir` (and `dir`) the bits `u+rwx`. A symbolic
/// link is not followed.
fn writable(dir: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if !meta.is_dir() {
        return Ok(());
    }
    let mode = meta.permissions().mode();
    if mode & 0o700 != 0o700 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode | 0o700))
            .map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        writable(&entry.path())?;
    }
    Ok(())
}

/// Remove `dir` and each file in it; no `dir` is no error.
pub fn clear(dir: &Path) -> Result<(), String> {
    if std::fs::symlink_metadata(dir).is_err() {
        return Ok(());
    }
    writable(dir).map_err(|e| format!("the work directory: {e}"))?;
    std::fs::remove_dir_all(dir).map_err(|e| format!("the work directory {}: {e}", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_with_no_write_bit_is_removed() {
        let dir = std::env::temp_dir().join(format!("podrunner-work-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/locked/deep")).unwrap();
        std::fs::write(dir.join("a/locked/deep/f"), "x").unwrap();
        std::fs::set_permissions(dir.join("a/locked/deep"), std::fs::Permissions::from_mode(0o500)).unwrap();
        std::fs::set_permissions(dir.join("a/locked"), std::fs::Permissions::from_mode(0o000)).unwrap();
        clear(&dir).unwrap();
        assert!(!dir.exists());
        assert!(clear(&dir).is_ok(), "no directory is no error");
    }
}
