//! The host directories of a start in the fake unit (T-480). A fake unit
//! has no root file system of its own: a mount is a path in the root of
//! the unit, and each variable that names the target names that path. A
//! read-write mount is a link to the host directory; a `:ro` mount is a
//! copy, so a write of the job never reaches the host directory.

use std::path::{Path, PathBuf};

use podrunner_unit::{UnitCode, UnitError};

/// The directory of the mounts in the root of a unit.
const MOUNTS: &str = ".mounts";

/// Put each mount of `mounts` in `root`, and point each value of `env`
/// under a target to its place.
pub(crate) fn place(root: &Path, mounts: &[(PathBuf, String)], env: &mut [(String, String)]) -> Result<(), UnitError> {
    let fail = |e: std::io::Error| UnitError::new(UnitCode::UnitStartFailed, format!("a mount: {}", e.kind()));
    for (i, (host, target)) in mounts.iter().enumerate() {
        let (guest, ro) = target.strip_suffix(":ro").map_or((target.as_str(), false), |g| (g, true));
        let at = root.join(MOUNTS).join(i.to_string());
        std::fs::create_dir_all(root.join(MOUNTS)).map_err(fail)?;
        if ro {
            copy(host, &at).map_err(fail)?;
        } else {
            std::os::unix::fs::symlink(host, &at).map_err(fail)?;
        }
        for (_, v) in env.iter_mut() {
            if let Some(rest) = v.strip_prefix(guest).filter(|r| r.is_empty() || r.starts_with('/')) {
                *v = format!("{}{rest}", at.display());
            }
        }
    }
    Ok(())
}

/// Copy the tree `from` to `to`; a link stays a link.
fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let (src, dst) = (e.path(), to.join(e.file_name()));
        let t = e.file_type()?;
        if t.is_dir() {
            copy(&src, &dst)?;
        } else if t.is_symlink() {
            std::os::unix::fs::symlink(std::fs::read_link(&src)?, &dst)?;
        } else if t.is_file() {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}
