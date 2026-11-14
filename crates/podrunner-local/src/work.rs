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

/// Copy each workspace of `work` (each entry whose name does not start
/// with `_`: not `_actions`, `_temp`, `_tool`) into `to`, which is new
/// (T-138). A symbolic link stays a link; the mode of each file stays.
pub fn export(work: &Path, to: &Path) -> Result<(), String> {
    clear(to)?;
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let Ok(entries) = std::fs::read_dir(work) else { return Ok(()) };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with('_') {
            copy(&entry.path(), &to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    let err = |e: std::io::Error| format!("the export of {}: {e}", from.display());
    let meta = std::fs::symlink_metadata(from).map_err(err)?;
    if meta.file_type().is_symlink() {
        std::os::unix::fs::symlink(std::fs::read_link(from).map_err(err)?, to).map_err(err)?;
    } else if meta.is_dir() {
        std::fs::create_dir_all(to).map_err(err)?;
        for entry in std::fs::read_dir(from).map_err(err)? {
            let entry = entry.map_err(err)?;
            copy(&entry.path(), &to.join(entry.file_name()))?;
        }
        // A directory with no write bit gets its mode after its files.
        std::fs::set_permissions(to, meta.permissions()).map_err(err)?;
    } else if meta.is_file() {
        std::fs::copy(from, to).map_err(err)?;
    }
    Ok(())
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

    #[test]
    fn the_export_copies_the_workspaces_only() {
        let base = std::env::temp_dir().join(format!("podrunner-export-{}", std::process::id()));
        let (work, to) = (base.join("_work"), base.join("out"));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(work.join("repo/repo/target")).unwrap();
        std::fs::create_dir_all(work.join("_temp")).unwrap();
        std::fs::write(work.join("repo/repo/target/app"), "bin").unwrap();
        std::fs::set_permissions(work.join("repo/repo/target/app"), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink("target/app", work.join("repo/repo/link")).unwrap();
        std::fs::write(work.join("_temp/x.sh"), "x").unwrap();
        std::fs::create_dir_all(to.join("stale")).unwrap();
        export(&work, &to).unwrap();
        assert_eq!(std::fs::read_to_string(to.join("repo/repo/target/app")).unwrap(), "bin");
        let mode = std::fs::metadata(to.join("repo/repo/target/app")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        assert_eq!(std::fs::read_link(to.join("repo/repo/link")).unwrap(), Path::new("target/app"));
        assert!(!to.join("_temp").exists() && !to.join("stale").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
