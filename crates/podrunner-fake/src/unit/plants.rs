//! The plants of a fake unit at its start: each makes the unit break one
//! rule, so that a conformance test proves that it finds the break.

use std::fs;
use std::path::Path;

use podrunner_unit::{StartSpec, UnitError};

use super::{FakeUnit, UnitPlant, host};

impl FakeUnit {
    /// The plants 19, 21, 22 and 24 at start.
    pub(super) fn plant_start(&self, spec: &StartSpec, root: &Path) -> Result<(), UnitError> {
        let first = spec.credential.files.first().map(|f| f.content.expose().to_string());
        if self.planted(UnitPlant::SharedHosts) {
            fs::create_dir_all(root.join("etc")).map_err(|e| host(e, "etc"))?;
            let _ = std::os::unix::fs::symlink(self.state_dir.join("hosts"), root.join("etc/hosts"));
        }
        if self.planted(UnitPlant::CredentialCopy)
            && let Some(text) = &first
        {
            fs::write(self.state_dir.join(format!("copy-{}", spec.unit)), text).map_err(|e| host(e, "a copy"))?;
        }
        if self.planted(UnitPlant::ProofCanary) || self.planted(UnitPlant::CredentialInContainers) {
            *self.seen.lock().expect("the credential seen") = first;
        }
        Ok(())
    }
}
