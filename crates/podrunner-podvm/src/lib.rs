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
//! `probe` maps `podvm probe --json` ([`probe`]); the start of a unit comes
//! next (T-023).

pub mod probe;

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
        let v = serde_json::from_slice(&out.stdout)
            .map_err(|_| UnitError::new(UnitCode::Unsupported, "podvm probe --json gives no JSON"))?;
        probe::capability(&v, std::env::consts::ARCH)
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

    #[test]
    fn a_missing_binary_is_a_host_error() {
        let e = Podvm { binary: "/nonexistent/podvm".into() }.probe().unwrap_err();
        assert_eq!(e.code, UnitCode::Host);
    }
}
