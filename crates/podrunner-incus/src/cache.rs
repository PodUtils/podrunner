//! The cache of `serve` in an instance (T-648): the one host directory of
//! a start that the kind mounts, at [`CACHE_MOUNT`].
//!
//! podvm `0.2.0-beta.40` makes it a `disk` device of the instance, with
//! `readonly=true` for `:ro`; the script of the instance refuses a `ro`
//! cache that is not a read-only mount. The instance has a map of ids: the
//! user of podrunner is no id in it, and a file that the instance writes
//! has an id of the map, which the user of podrunner cannot remove under a
//! directory of mode 0755. So the record lists `ro` only, and `serve` gives
//! each job of the kind `ro`; the user of the image reads a file through
//! its mode bits for others.

use std::path::{Path, PathBuf};

use podrunner_unit::{UnitCode, UnitError};

/// Where the instance sees the cache of its job.
pub const CACHE_MOUNT: &str = "/podrunner-cache";

/// The check of a `ro` cache in the instance, before the payload: the
/// last mount at [`CACHE_MOUNT`] is read-only.
pub const RO_CHECK: &str = r#"# The cache of the job is read-only in the instance (T-648).
if ! awk '$2 == "/podrunner-cache" { n++; ro = ("," $4 ",") ~ /,ro,/ } END { exit !(n && ro) }' /proc/mounts; then
  echo "podrunner: the read-only cache is not a read-only mount of the instance" >&2
  exit 96
fi
"#;

/// The cache of a start: its host directory, and true when read-only.
/// Each other host directory is refused: the instance sees no other part
/// of the host.
pub fn of(mounts: &[(PathBuf, String)]) -> Result<Option<(&Path, bool)>, UnitError> {
    let refuse = |why: String| Err(UnitError::new(UnitCode::UnitStartFailed, why));
    let (host, path) = match mounts {
        [] => return Ok(None),
        [(host, path)] => (host, path),
        _ => return refuse(format!("this kind mounts one host directory, the cache at {CACHE_MOUNT} (T-138, T-648)")),
    };
    let (guest, ro) = path.strip_suffix(":ro").map_or((path.as_str(), false), |g| (g, true));
    if guest != CACHE_MOUNT {
        return refuse(format!(
            "this kind mounts no host directory at {guest}: only the cache at {CACHE_MOUNT} (T-138, T-648)"
        ));
    }
    if !host.is_absolute() {
        return refuse(format!("the host path of the cache {} is not absolute", host.display()));
    }
    podrunner_unit::mounts::host_path(host, UnitCode::UnitStartFailed)?;
    Ok(Some((host, ro)))
}
