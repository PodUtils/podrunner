//! The check of a driver file (podvm, podnet, a sidecar, a program) at its
//! use: a file of another user, or one under an open directory, could be
//! replaced by another program.

/// Check a driver file at its use (#263): a regular file that this user
/// can run, and the file and each directory above it belong to root or to
/// this uid, and the group and the others cannot write them (a directory
/// with the sticky bit that root owns, as `/tmp`, can be open).
pub fn check_driver(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: getuid has no failure and no side effect.
    let me = unsafe { libc::getuid() };
    let m = std::fs::metadata(path).map_err(|e| format!("{}: {}", path.display(), e.kind()))?;
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| "a NUL in the path".to_string())?;
    // SAFETY: access on a path that this function owns.
    if !m.is_file() || unsafe { libc::access(c.as_ptr(), libc::X_OK) } != 0 {
        return Err(format!("{} is not a file that this user can run", path.display()));
    }
    // Each real directory above it: a link does not hide one.
    let abs = std::fs::canonicalize(path).map_err(|e| format!("{}: {}", path.display(), e.kind()))?;
    for p in abs.ancestors() {
        let m = std::fs::metadata(p).map_err(|e| format!("{}: {}", p.display(), e.kind()))?;
        if m.uid() != 0 && m.uid() != me {
            return Err(format!("{} belongs to another user", p.display()));
        }
        let sticky_root = p != abs && m.uid() == 0 && m.mode() & 0o1000 != 0;
        if m.mode() & 0o022 != 0 && !sticky_root {
            return Err(format!("{} is writable by the group or the others", p.display()));
        }
    }
    Ok(())
}
