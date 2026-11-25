//! The plants of a fake unit at its start: each makes the unit break one
//! rule, so that a conformance test proves that it finds the break.

use std::fs;
use std::path::Path;

use podrunner_unit::{StartSpec, UnitError};

use super::{FakeUnit, host};

/// The planted defects of the fake unit: each one fails the unit
/// conformance test of its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitPlant {
    /// 1: each probe gives another record.
    ProbeDrifts,
    /// 2: a tag is accepted.
    AcceptTag,
    /// 3: destroy kills the top process only.
    KillTopOnly,
    /// 4: a foreign platform is accepted.
    AcceptForeign,
    /// 5: each unit gets the same root.
    SharedRoot,
    /// 6: the credential files have mode 0644.
    LooseMode,
    /// 7: the credential goes into the environment.
    CredentialInEnv,
    /// 8: wait gives exit code 0.
    WrongCode,
    /// 9: signal does nothing.
    DeafSignal,
    /// 10: destroy kills nothing.
    NoKill,
    /// 11: destroy keeps the files.
    KeepFiles,
    /// 12: destroy counts nothing (`unknown`).
    UnknownResidue,
    /// 13: a container starts with no route.
    ContainerWithNoRoute,
    /// 14: exec accepts stdin.
    AcceptStdin,
    /// 15: a second destroy fails.
    DestroyOnce,
    /// 16: a unit id in use starts again.
    StartTwice,
    /// 17: probe claims a verified tree kill, and destroy kills the top
    /// process only.
    FalseTreeKill,
    /// 18: probe writes a file in the state directory.
    ProbeWritesFile,
    /// 19: each unit gets one shared `/etc/hosts`.
    SharedHosts,
    /// 20: the processes of a container do not carry the tag of the unit.
    UntaggedContainer,
    /// 21: start keeps a copy of the credential outside the unit root.
    CredentialCopy,
    /// 22: probe gives the credential of the last unit in `boundary_proof`.
    ProofCanary,
    /// 23: probe leaves a process.
    ProbeSpawns,
    /// 24: the credential goes into the environment of each container.
    CredentialInContainers,
    /// 25: no shim: destroy finds the processes by their tag only.
    TagOnly,
    /// 26: destroy counts 0 processes after its shim was killed.
    ShimDeathIgnored,
    /// 27: the record claims `confined`, and no rule applies.
    ClaimsConfined,
    /// 28: a warm unit gets the credential files of the last claim.
    WarmCredential,
    /// 29: a claimed unit takes a second claim.
    ClaimTwice,
    /// 30: the destroy of an unclaimed warm unit keeps its root.
    WarmLeak,
    /// 31: a claim runs a fixed argv, not the argv of the claim.
    ClaimBootArgv,
}

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
