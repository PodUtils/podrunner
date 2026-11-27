//! The length of a Unix socket path (T-738). `sun_path` holds 108 bytes
//! with its NUL, so a socket path has at most 107 bytes. A socket that a
//! driver, podnet or a guest reaches by its path must fit there: a bind or
//! a connect of a longer path fails with no path and no fix.

use std::path::{Path, PathBuf};

/// The longest path of a Unix socket: `sun_path` less its NUL.
pub const SUN_PATH_MAX: usize = 107;

/// The fix of a socket path that a long `TMPDIR` makes too long.
pub const TMPDIR_FIX: &str = "set TMPDIR to a shorter path";

/// Nothing when `path` fits `sun_path`; else the path, its length and
/// `fix`.
pub fn fits(path: &Path, fix: &str) -> Result<(), String> {
    let n = path.as_os_str().len();
    if n <= SUN_PATH_MAX {
        return Ok(());
    }
    Err(format!(
        "the socket path {} has {n} bytes, and a Unix socket path takes at most {SUN_PATH_MAX}; fix: {fix}",
        path.display()
    ))
}

/// A directory `name` for sockets with paths of `room` more bytes:
/// [`short_dir_in`] of the temporary directory.
pub fn short_dir(name: &str, room: usize) -> PathBuf {
    short_dir_in(&std::env::temp_dir(), Path::new("/tmp"), name, room)
}

/// `tmp/NAME` when a socket path of `room` more bytes fits there; else
/// `slash_tmp/NAME` when it fits there and `slash_tmp` is a directory;
/// else `tmp/NAME`, and [`fits`] names the path and the fix. The
/// directory holds sockets only, so a `/tmp` that the user moved away
/// from gets no data. The caller makes it at mode 0700 and refuses a link
/// or a directory of another user at its name: `/tmp` is shared.
pub fn short_dir_in(tmp: &Path, slash_tmp: &Path, name: &str, room: usize) -> PathBuf {
    let fit = |d: &Path| d.as_os_str().len() + room <= SUN_PATH_MAX;
    let first = tmp.join(name);
    if fit(&first) {
        return first;
    }
    let other = slash_tmp.join(name);
    if fit(&other) && slash_tmp.is_dir() { other } else { first }
}

/// Make `root` a directory of this user at 0700, or check that it is one:
/// a link, a file or a directory of another user is refused (T-407). A
/// root in a shared directory can have a name that another user made
/// first; each socket in it would then be in that user's directory.
pub fn own_root(root: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    match std::fs::symlink_metadata(root) {
        Ok(m) => {
            // SAFETY: getuid has no failure.
            let me = unsafe { libc::getuid() };
            if !m.is_dir() || m.uid() != me {
                return Err(std::io::Error::other(format!("{}: not a directory of this user", root.display())));
            }
            std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new().recursive(true).mode(0o700).create(root)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-738: a path of 107 bytes fits; one more byte is refused with the
    /// path, its length and the fix.
    #[test]
    fn a_socket_path_past_sun_path_is_named_with_its_fix() {
        let edge = format!("/{}", "s".repeat(SUN_PATH_MAX - 1));
        assert!(fits(Path::new(&edge), TMPDIR_FIX).is_ok());
        let long = format!("{edge}s");
        let e = fits(Path::new(&long), TMPDIR_FIX).unwrap_err();
        assert!(e.starts_with(&format!("the socket path {long} has 108 bytes")), "{e}");
        assert!(e.ends_with("fix: set TMPDIR to a shorter path"), "{e}");
    }

    /// T-738: a short `TMPDIR` keeps the directory; a `TMPDIR` of 100 bytes
    /// gives `/tmp`; with no `/tmp`, or with no room there, the directory
    /// stays in `TMPDIR`, where `fits` names it.
    #[test]
    fn a_long_tmpdir_gives_a_short_directory_in_slash_tmp() {
        let room = "/u-18446744073709551615/egress-inner.sock".len();
        let (short, long) = (Path::new("/t"), PathBuf::from(format!("/{}", "t".repeat(99))));
        // A short directory that exists on each host, as `/tmp`.
        let slash = Path::new("/");
        assert_eq!(short_dir_in(short, slash, "pr-1", room), short.join("pr-1"));
        assert_eq!(short_dir_in(&long, slash, "pr-1", room), slash.join("pr-1"));
        let none = Path::new("/nonexistent/podrunner-tmp");
        assert_eq!(short_dir_in(&long, none, "pr-1", room), long.join("pr-1"), "no /tmp");
        assert_eq!(short_dir_in(&long, slash, "pr-1", SUN_PATH_MAX), long.join("pr-1"), "no room in /tmp");
        // The edge: a socket of `room` bytes at 107 bytes fits.
        let edge = PathBuf::from(format!("/{}", "t".repeat(SUN_PATH_MAX - room - 1 - "/pr-1".len())));
        assert_eq!(short_dir_in(&edge, none, "pr-1", room).as_os_str().len() + room, SUN_PATH_MAX);
        let past = PathBuf::from(format!("{}t", edge.display()));
        assert_eq!(short_dir_in(&past, slash, "pr-1", room), slash.join("pr-1"));
    }
}
