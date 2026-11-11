//! The root of a running podvm unit (T-067). podvm `v0.2.0-beta.3` ran a
//! unit in the rootfs that `podvm extract` names. From beta.4 podvm copies the
//! image into a tree for each container at `run` (PodUtils/podvm@T-1709)
//! and names it in `podvm inspect --format '{{.RootfsPath}}'`; the runner
//! then writes in that tree. The driver takes the tree that `inspect`
//! names, so `observe` and the volume sources of the hooks follow the
//! unit. The copy takes the credential files written before `run`.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use podrunner_unit::{UnitCode, UnitError};

use super::{PodvmUnit, RUNNER_ROOT, WORKDIR};

impl PodvmUnit {
    /// True when podvm says that the launcher of the container `name` was
    /// killed (`State` `dead`): its orphans went to init, out of reach of
    /// podvm and of the driver (design test 15).
    pub(super) fn launcher_died(&self, store: &Path, name: &str) -> bool {
        let Ok(out) = Command::new(&self.podvm.binary)
            .args(["inspect", name])
            .env("PODVM_STORE", store)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            return false;
        };
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
        let one = if v.is_array() { &v[0] } else { &v };
        one["State"] == "dead" || one["State"]["Status"] == "dead"
    }

    /// `RootfsPath` of the container `name` in `store`, when podvm names
    /// a directory.
    pub(super) fn inspected_root(&self, store: &Path, name: &str) -> Option<PathBuf> {
        let out = Command::new(&self.podvm.binary)
            .args(["inspect", "--format", "{{.RootfsPath}}", name])
            .env("PODVM_STORE", store)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let path = PathBuf::from(text.lines().rev().find(|l| !l.trim().is_empty())?.trim());
        (out.status.success() && path.is_dir()).then_some(path)
    }
}

/// The root of a running unit: the tree that `inspect` names, else the
/// extracted rootfs. Each credential file `names` must be in the runner
/// root of it at mode 0600: no unit runs with a credential that it cannot
/// read, or that another user can.
pub(super) fn root_of_unit(extracted: &Path, inspected: Option<PathBuf>, names: &[&str]) -> Result<PathBuf, UnitError> {
    let root = inspected.unwrap_or_else(|| extracted.to_path_buf());
    let runner_root = root.join(WORKDIR.trim_start_matches('/')).join(RUNNER_ROOT);
    for name in names {
        let mode = std::fs::metadata(runner_root.join(name)).map(|m| m.permissions().mode() & 0o777);
        if mode.ok() != Some(0o600) {
            return Err(UnitError::new(
                UnitCode::UnitStartFailed,
                format!("the credential file {name} is not in the tree of the unit at mode 0600"),
            ));
        }
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(base: &Path, name: &str, mode: Option<u32>) -> PathBuf {
        let root = base.join(name);
        let runner_root = root.join(WORKDIR.trim_start_matches('/')).join(RUNNER_ROOT);
        std::fs::create_dir_all(&runner_root).unwrap();
        if let Some(m) = mode {
            let f = runner_root.join(".runner");
            std::fs::write(&f, "{}").unwrap();
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(m)).unwrap();
        }
        root
    }

    #[test]
    fn the_rootfs_of_inspect_wins_and_holds_the_credential() {
        let base = std::env::temp_dir().join(format!("podrunner-rootfs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let extracted = tree(&base, "extracted", Some(0o600));
        // beta.3: no other tree; the extracted rootfs.
        assert_eq!(root_of_unit(&extracted, None, &[".runner"]).unwrap(), extracted);
        // A tree for each container: the driver follows it.
        let copy = tree(&base, "copy", Some(0o600));
        assert_eq!(root_of_unit(&extracted, Some(copy.clone()), &[".runner"]).unwrap(), copy);
        // A copy that lost the file, or its mode, fails the start.
        let empty = tree(&base, "empty", None);
        let e = root_of_unit(&extracted, Some(empty), &[".runner"]).unwrap_err();
        assert_eq!(e.code, UnitCode::UnitStartFailed);
        let loose = tree(&base, "loose", Some(0o644));
        assert!(root_of_unit(&extracted, Some(loose), &[".runner"]).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
