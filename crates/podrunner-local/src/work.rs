//! The work directory of a runner directory (T-146). Each job starts in a
//! fresh `_work`. A step can leave a directory with no write bit, which
//! `remove_dir_all` cannot empty: the next job would then run in the stale
//! tree with no message. So each directory gets its write bit back before
//! the removal, and a failure names the path.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

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

/// The work directory of the runner directory `dir`: `dir/_work`, as the
/// runner has it. A path with a component that starts with `.` (the
/// installer puts the runner in `~/.local/share`) breaks jobs: the `send`
/// module of Node refuses such a path with 404, and the tests of express
/// fail (T-255). Then the work directory is
/// `BASE/podrunner-work/NAME-HASH`: BASE is the path before the first such
/// component, NAME the name of `dir`, HASH a hash of the path of `dir`.
pub fn host_work(dir: &Path) -> std::path::PathBuf {
    use std::path::Component;
    let abs = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    let dot = |c: &Component| matches!(c, Component::Normal(n) if n.to_string_lossy().starts_with('.'));
    let Some(at) = abs.components().position(|c| dot(&c)) else { return dir.join("_work") };
    let base: std::path::PathBuf = abs.components().take(at).collect();
    // FNV-1a: the same path gives the same directory in each version.
    let hash = abs
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3));
    let name = abs.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    base.join("podrunner-work").join(format!("{}-{:08x}", name.trim_start_matches('.'), hash as u32))
}

/// Make `dir` new, at 0700, owned by this user: a directory of the same name
/// that this user left goes first; one that another user made fails, as
/// its links and its files are not ours (T-406, T-407).
pub fn private_dir(dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    if let Ok(m) = std::fs::symlink_metadata(dir) {
        // SAFETY: getuid has no failure.
        let me = unsafe { libc::getuid() };
        if m.uid() != me || m.file_type().is_symlink() {
            return Err(format!("{}: there already, and not a directory of this user: remove it", dir.display()));
        }
        std::fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::DirBuilder::new().mode(0o700).create(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// The work directory of a job with `--export` (T-408): `outer` new at
/// 0700 (a name of another user, or a link, is refused), and in it `work`
/// at 0777, which the runner user of the unit writes through its mount. No
/// other user of the host can reach `work`: the parent is closed.
pub fn export_dir(outer: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    private_dir(outer)?;
    let work = outer.join("work");
    std::fs::create_dir(&work)
        .and_then(|()| std::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o777)))
        .map_err(|e| format!("{}: {e}", work.display()))?;
    Ok(work)
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
    /// T-408: the work directory of `--export` is 0777 in a parent at 0700
    /// of this user; a link at the name of the parent is refused.
    #[test]
    fn the_export_directory_is_open_only_through_a_closed_parent() {
        use std::os::unix::fs::PermissionsExt;
        use std::path::Path;
        let base = std::env::temp_dir().join(format!("podrunner-export-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("theirs")).unwrap();
        let outer = base.join("job");
        let work = super::export_dir(&outer).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!((mode(&outer), mode(&work)), (0o700, 0o777));
        assert_eq!(work.parent(), Some(outer.as_path()));
        std::fs::remove_dir_all(&outer).unwrap();
        std::os::unix::fs::symlink(base.join("theirs"), &outer).unwrap();
        assert!(super::export_dir(&outer).is_err());
        assert!(std::fs::read_dir(base.join("theirs")).unwrap().next().is_none(), "nothing through the link");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// T-406, T-407: a private directory is new and 0700; a link there is
    /// refused.
    #[test]
    fn a_private_directory_is_new_and_ours() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("podrunner-private-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let d = base.join("d");
        std::fs::create_dir_all(d.join("old")).unwrap();
        super::private_dir(&d).unwrap();
        assert_eq!(std::fs::metadata(&d).unwrap().permissions().mode() & 0o777, 0o700);
        assert!(!d.join("old").exists(), "a directory of this user is made new");
        let link = base.join("link");
        std::os::unix::fs::symlink(&base, &link).unwrap();
        assert!(super::private_dir(&link).unwrap_err().contains("not a directory of this user"));
        let _ = std::fs::remove_dir_all(&base);
    }

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

    /// The work directory has no component with a dot (T-255).
    #[test]
    fn the_work_directory_has_no_dot_component() {
        assert_eq!(host_work(Path::new("/srv/runner")), Path::new("/srv/runner/_work"));
        let w = host_work(Path::new("/home/u/.local/share/podrunner/runner/2.338.2"));
        assert!(w.starts_with("/home/u/podrunner-work/"), "{}", w.display());
        let name = w.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("2.338.2-") && name.len() == "2.338.2-".len() + 8, "{name}");
        assert_ne!(w, host_work(Path::new("/home/u/.local/share/podrunner/runner/x/2.338.2")), "one for each dir");
        assert_eq!(w, host_work(Path::new("/home/u/.local/share/podrunner/runner/2.338.2")), "stable");
    }
}
