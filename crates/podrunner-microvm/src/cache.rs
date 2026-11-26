//! The cache of `serve` in a guest (T-648): the one host directory of a
//! start that the kind mounts, at [`CACHE_MOUNT`].
//!
//! podvm `0.2.0-beta.40` shares it over 9p with `security_model=none`: the
//! host user owns each file that the guest writes, so the host removes the
//! cache; a `:ro` share is `readonly=on` in QEMU and a read-only mount in
//! the guest, and the mount check of the guest refuses it writable. The
//! guest checks each access against the host owner, so the user of the
//! image writes a share only when its uid is the uid of the host user: the
//! record lists `ro` only, and `serve` gives each job of the kind `ro`.

use std::path::{Path, PathBuf};

use podrunner_unit::{UnitCode, UnitError};

/// Where the guest sees the cache of its job.
pub const CACHE_MOUNT: &str = "/podrunner-cache";

/// The cache of a start: its host directory, and true when read-only.
/// Each other host directory is refused: the guest sees no other part of
/// the host.
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
    nine_p(host)?;
    Ok(Some((host, ro)))
}

/// podvm puts a host path into a `-virtfs` option of QEMU, which reads a
/// `,` as the start of the next option.
pub(crate) fn nine_p(host: &Path) -> Result<(), UnitError> {
    if host.as_os_str().as_encoded_bytes().contains(&b',') {
        return Err(UnitError::new(
            UnitCode::UnitStartFailed,
            format!("the host path {} has a ',', which a 9p share of podvm cannot hold", host.display()),
        ));
    }
    Ok(())
}
