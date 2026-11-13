//! The podvm unit driver (GitHub #23). podvm is the only unit kind for a
//! host with no root. This crate is the only code that knows the argv of
//! podvm: a flag rename in a beta of podvm breaks this crate and its test,
//! not podrunner at run time.
//!
//! The driver uses the released podvm binary as it is. Its plan, from the
//! measured facts of GitHub #23: a store of its own for each unit
//! (`PODVM_STORE`), the image loaded into it from a saved tar; a tag in the
//! environment of each process of the unit, and a scan of `/proc` for the
//! tree kill, because a grandchild outlives `podvm stop` and `rm -f`;
//! `DOTNET_EnableWriteXorExecute=0` and no `fsize` limit; the rung read
//! from the run banner, not from a request; no `--unsafe-host-paths`.
//! `probe` maps `podvm probe --json` ([`probe`]); [`PodvmUnit`] is the
//! driver.

pub mod probe;
mod unit;

pub use unit::PodvmUnit;

use std::path::PathBuf;
use std::process::Command;

use podrunner_schema::Capability;
use podrunner_unit::{UnitCode, UnitError};

/// A podvm binary on this host.
#[derive(Debug, Clone)]
pub struct Podvm {
    pub binary: PathBuf,
}

impl Podvm {
    /// Run `podvm probe --json` and map it.
    pub fn probe(&self) -> Result<Capability, UnitError> {
        probe::capability(&self.probe_json()?, std::env::consts::ARCH)
    }

    /// The version of this podvm, refused below `lowest` or at
    /// [`probe::BEYOND`] and later: a kind that needs a newer podvm fails
    /// by name at `serve`, not each job at its start (T-110).
    pub fn require(&self, lowest: &str) -> Result<String, UnitError> {
        let v = self.probe_json()?;
        let unsupported = |why: String| UnitError::new(UnitCode::Unsupported, why);
        let version = v["podvm"].as_str().ok_or_else(|| unsupported("the probe names no podvm version".into()))?;
        let have =
            probe::Version::parse(version).ok_or_else(|| unsupported(format!("podvm {version:?} is not a version")))?;
        let low = probe::Version::parse(lowest).ok_or_else(|| unsupported(format!("{lowest:?} is not a version")))?;
        let beyond = probe::Version::parse(probe::BEYOND).expect("BEYOND");
        if have < low || !probe::before(&have, &beyond) {
            return Err(unsupported(format!(
                "podvm {version}: this kind needs {lowest} to {} (exclusive)",
                probe::BEYOND
            )));
        }
        Ok(version.to_string())
    }

    fn probe_json(&self) -> Result<serde_json::Value, UnitError> {
        let out = Command::new(&self.binary)
            .args(["probe", "--json"])
            .output()
            .map_err(|e| UnitError::new(UnitCode::Host, format!("podvm probe: {}", e.kind())))?;
        if !out.status.success() {
            return Err(UnitError::new(
                UnitCode::Host,
                format!("podvm probe: exit {}", out.status.code().unwrap_or(-1)),
            ));
        }
        serde_json::from_slice(&out.stdout)
            .map_err(|_| UnitError::new(UnitCode::Unsupported, "podvm probe --json gives no JSON"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With the released podvm on this host (PODRUNNER_PODVM names it), the
    /// probe maps. The test is ignored by default: CI has no podvm.
    #[test]
    #[ignore]
    fn the_probe_of_a_real_podvm_maps() {
        let binary = PathBuf::from(std::env::var_os("PODRUNNER_PODVM").expect("PODRUNNER_PODVM"));
        let c = Podvm { binary }.probe().unwrap();
        assert_eq!(c.kind, "podvm");
    }

    /// A fake podvm whose probe names `version`.
    fn fake(version: &str) -> (Podvm, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("podrunner-podvm-require-{version}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("podvm");
        std::fs::write(&f, format!("#!/bin/sh\necho '{{\"podvm\":\"{version}\",\"rung\":\"userland\"}}'\n")).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        (Podvm { binary: f }, d)
    }

    #[test]
    fn a_kind_refuses_a_podvm_below_its_lowest_version() {
        for (have, ok) in [("0.2.0-beta.9", false), ("0.2.0-beta.10", true), ("0.2.0", true), ("0.3.0-beta.1", false)] {
            let (p, d) = fake(have);
            let got = p.require("0.2.0-beta.10");
            let _ = std::fs::remove_dir_all(&d);
            assert_eq!(got.is_ok(), ok, "{have}: {got:?}");
            if let Err(e) = got {
                assert_eq!(e.code, UnitCode::Unsupported);
                assert!(e.to_string().contains(have) && e.to_string().contains("0.2.0-beta.10"), "{e}");
            }
        }
    }

    #[test]
    fn a_missing_binary_is_a_host_error() {
        let e = Podvm { binary: "/nonexistent/podvm".into() }.probe().unwrap_err();
        assert_eq!(e.code, UnitCode::Host);
    }
}
