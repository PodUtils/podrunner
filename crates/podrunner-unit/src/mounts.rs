//! The host paths of a `-v SRC:DST[:ro]` and a `--podvm-channel SRC:...` of
//! podvm (#275): the colon separates the parts, and podvm has no escape of
//! it, so a host path with a colon would be read as another source and
//! target. A driver checks each host path before it builds the arguments.

use std::path::Path;

use crate::{UnitCode, UnitError};

/// Refuse a host path that a `-v` of podvm cannot hold: one with a colon,
/// or one that is not UTF-8.
pub fn host_path(p: &Path, code: UnitCode) -> Result<(), UnitError> {
    match p.to_str() {
        Some(s) if !s.contains(':') => Ok(()),
        _ => Err(UnitError::new(
            code,
            format!(
                "the host path {} has a ':' or is not UTF-8, which a -v of podvm cannot hold: set TMPDIR (or the path of the option) to a path with no ':'",
                p.display()
            ),
        )),
    }
}

/// Each host path of a start: the socket directory and each mount.
pub fn spec(spec: &crate::StartSpec) -> Result<(), UnitError> {
    let hosts = spec.sockets.iter().map(|d| d.as_path()).chain(spec.mounts.iter().map(|(h, _)| h.as_path()));
    hosts.into_iter().try_for_each(|h| host_path(h, UnitCode::UnitStartFailed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_path_with_a_colon_is_refused() {
        for ok in ["/tmp/podrunner-run-units-1-0/u-1/work", "/home/runner", "/a b/c"] {
            assert!(host_path(Path::new(ok), UnitCode::UnitStartFailed).is_ok(), "{ok}");
        }
        for bad in ["/tmp/a:b", "/tmp/a:b/cred", ":", "/x/y:"] {
            let e = host_path(Path::new(bad), UnitCode::UnitStartFailed).unwrap_err();
            assert!(e.code == UnitCode::UnitStartFailed && e.detail.contains("set TMPDIR"), "{bad}: {e}");
        }
        use std::os::unix::ffi::OsStrExt;
        let raw = Path::new(std::ffi::OsStr::from_bytes(b"/tmp/\xff"));
        assert!(host_path(raw, UnitCode::ContainerOptionRefused).is_err(), "not UTF-8");
    }

    /// A start with a mount or a socket directory with a colon is refused.
    #[test]
    fn a_start_with_a_colon_in_a_host_path_is_refused() {
        let base = crate::StartSpec {
            unit: crate::UnitId::parse("u-1").unwrap(),
            image: crate::Prepared { digest: format!("sha256:{}", "0".repeat(64)), platform: "linux/amd64".into() },
            argv: vec!["x".into()],
            env: Default::default(),
            credential: Default::default(),
            sockets: Some("/tmp/s".into()),
            mounts: vec![("/tmp/w".into(), "/home/runner/_work".into())],
            limits: Default::default(),
        };
        assert!(spec(&base).is_ok());
        let colon = crate::StartSpec { sockets: Some("/tmp/a:b/s".into()), ..base.clone() };
        assert!(spec(&colon).unwrap_err().detail.contains("/tmp/a:b/s"));
        let mount = crate::StartSpec { mounts: vec![("/tmp/t:c".into(), "/opt/hostedtoolcache".into())], ..base };
        assert!(spec(&mount).is_err());
    }
}
