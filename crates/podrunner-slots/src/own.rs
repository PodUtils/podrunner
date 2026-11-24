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

/// Whether `root` is a directory of this user that is no link.
fn is_ours(root: &Path) -> bool {
    // SAFETY: getuid has no failure.
    let me = unsafe { libc::getuid() };
    std::fs::symlink_metadata(root).is_ok_and(|m| m.is_dir() && m.uid() == me)
}

/// Remove the socket directory of `unit`. When the root is the short
/// directory of the temporary directory, remove it too when no unit is
/// left in it: nothing else removes it. A root that is not ours is not
/// touched (T-620).
pub(crate) fn end_socket_dir(state: &Path, unit: &podrunner_schema::UnitId) {
    let root = crate::run::egress_dir(state);
    if !is_ours(&root) {
        return;
    }
    let _ = std::fs::remove_dir_all(root.join(unit.as_str()));
    if root != state.join("egress") {
        // Only an empty directory goes: a unit of the same state stays.
        let _ = std::fs::remove_dir(&root);
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

    /// A state path that takes the short root, and the removal of what
    /// the test leaves at the name of that root, also after a panic.
    struct Deep(std::path::PathBuf);

    impl Deep {
        fn new(tag: &str) -> Deep {
            let name = format!("podrunner-end-{tag}-{}-{}", std::process::id(), "d".repeat(90));
            let d = Deep(std::path::PathBuf::from("/nowhere").join(name));
            assert_ne!(crate::run::egress_dir(&d.0), d.0.join("egress"), "the short root");
            let _ = std::fs::remove_dir_all(crate::run::egress_dir(&d.0));
            d
        }
        fn root(&self) -> std::path::PathBuf {
            crate::run::egress_dir(&self.0)
        }
    }

    impl Drop for Deep {
        fn drop(&mut self) {
            let root = self.root();
            if std::fs::symlink_metadata(&root).is_ok_and(|m| m.is_symlink()) {
                let _ = std::fs::remove_file(&root);
            } else {
                let _ = std::fs::remove_dir_all(&root);
            }
        }
    }

    /// A directory that goes at the drop, also after a panic.
    struct Rm(std::path::PathBuf);

    impl Drop for Rm {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn unit(n: &str) -> podrunner_schema::UnitId {
        podrunner_schema::UnitId::parse(n).unwrap()
    }

    /// T-620: the short root goes with its last unit directory.
    #[test]
    fn the_short_root_goes_when_no_unit_directory_is_left_in_it() {
        let d = Deep::new("last");
        own_root(&d.root()).unwrap();
        for u in ["u-1", "u-2"] {
            std::fs::create_dir(d.root().join(u)).unwrap();
        }
        end_socket_dir(&d.0, &unit("u-1"));
        assert!(!d.root().join("u-1").exists());
        assert!(d.root().join("u-2").exists(), "the other unit keeps its directory and the root");
        end_socket_dir(&d.0, &unit("u-2"));
        assert!(!d.root().exists(), "the empty root is gone");
        end_socket_dir(&d.0, &unit("u-2"));
    }

    /// The root of `STATE/egress` stays: it is inside the state directory.
    #[test]
    fn the_root_in_the_state_directory_stays() {
        // A short relative path: a path in a long temp directory would take
        // the short root.
        let tmp = std::path::PathBuf::from(format!("podrunner-end-state-{}", std::process::id()));
        let _tmp = Rm(tmp.clone());
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("egress/u-1")).unwrap();
        assert_eq!(crate::run::egress_dir(&tmp), tmp.join("egress"));
        end_socket_dir(&tmp, &unit("u-1"));
        assert!(tmp.join("egress").is_dir());
        assert!(!tmp.join("egress/u-1").exists());
    }

    /// A link at the name of the root is not followed or removed.
    #[test]
    fn a_link_at_the_name_of_the_root_is_not_touched() {
        let d = Deep::new("link");
        let theirs = std::env::temp_dir().join(format!("podrunner-end-theirs-{}", std::process::id()));
        let _theirs = Rm(theirs.clone());
        std::fs::create_dir_all(theirs.join("u-1")).unwrap();
        std::os::unix::fs::symlink(&theirs, d.root()).unwrap();
        end_socket_dir(&d.0, &unit("u-1"));
        assert!(theirs.join("u-1").is_dir(), "the target of the link keeps its content");
        assert!(std::fs::symlink_metadata(d.root()).unwrap().is_symlink(), "the link stays");
        std::fs::remove_dir(theirs.join("u-1")).unwrap();
        end_socket_dir(&d.0, &unit("u-1"));
        assert!(std::fs::symlink_metadata(d.root()).unwrap().is_symlink(), "a link to an empty directory stays");
    }

    /// A directory of another user is refused by `own_root`, and
    /// `end_socket_dir` does not touch it. Only root can make one.
    #[test]
    fn a_directory_of_another_user_is_refused_and_not_touched() {
        // SAFETY: getuid has no failure.
        if unsafe { libc::getuid() } != 0 {
            return;
        }
        let d = Deep::new("other");
        std::fs::create_dir_all(d.root().join("u-1")).unwrap();
        std::os::unix::fs::chown(d.root(), Some(65534), None).unwrap();
        assert!(own_root(&d.root()).unwrap_err().to_string().contains("not a directory of this user"));
        end_socket_dir(&d.0, &unit("u-1"));
        assert!(d.root().join("u-1").is_dir(), "the directory of another user keeps its content");
    }
}
