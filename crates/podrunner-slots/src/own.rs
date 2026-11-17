//! The root of the socket directories of the units (T-407): `STATE/egress`,
//! or a short directory in the shared temporary directory when a socket
//! path there would be too long. In the shared directory another user can
//! make the name first; each socket of a unit would then be in that user's
//! directory, or behind a link.

use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::Path;

/// Make `root` a directory of this user at 0700, or check that it is one:
/// a link, a file or a directory of another user is refused.
pub(crate) fn own_root(root: &Path) -> std::io::Result<()> {
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

    #[test]
    fn the_root_of_the_sockets_is_a_directory_of_this_user() {
        let tmp = std::env::temp_dir().join(format!("podrunner-own-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("theirs")).unwrap();
        let new = tmp.join("a/b");
        own_root(&new).unwrap();
        assert_eq!(std::fs::metadata(&new).unwrap().permissions().mode() & 0o777, 0o700);
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o777)).unwrap();
        own_root(&new).unwrap();
        assert_eq!(std::fs::metadata(&new).unwrap().permissions().mode() & 0o777, 0o700, "made private again");
        let link = tmp.join("link");
        std::os::unix::fs::symlink(tmp.join("theirs"), &link).unwrap();
        assert!(own_root(&link).unwrap_err().to_string().contains("not a directory of this user"));
        std::fs::write(tmp.join("file"), "").unwrap();
        assert!(own_root(&tmp.join("file")).is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
