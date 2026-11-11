//! The egress hold of a podvm unit (T-065): `podvm run -d` of the unit,
//! `run -d` of each container and each `exec` run under `podnet pod
//! --backend seccomp -x http://unix:EGRESS`, so that each TCP connect of
//! the unit goes to its egress socket. Measured on the supervise rung of
//! podvm `v0.2.0-beta.3` with podnet `v0.4.0-beta.3` and no root
//! (`scripts/measure-podnet-pod.sh`): the payload is held for its whole
//! life after `podnet pod` returns; an `exec` is held only under a `podnet
//! pod` of its own. The level is `advisory`: loopback TCP and unix sockets
//! are not held (`podnet pod doctor`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use podrunner_schema::EgressHold;
use podrunner_unit::{EGRESS_SOCKET, StartSpec, UnitCode, UnitError};
use serde_json::Value;

use super::{PodvmUnit, host};

impl PodvmUnit {
    /// Hold the egress of each unit with `podnet` (T-065). Refused when
    /// `podnet pod doctor --json` gives no seccomp hold of at least
    /// `advisory` on this host.
    pub fn with_podnet(mut self, podnet: &Path) -> Result<PodvmUnit, UnitError> {
        let out = Command::new(podnet)
            .args(["pod", "doctor", "--json"])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| host("podnet pod doctor", e))?;
        let level = seccomp_level(&out.stdout)
            .ok_or_else(|| UnitError::new(UnitCode::Host, "podnet pod doctor gives no seccomp level"))?;
        if !matches!(level.as_str(), "advisory" | "enforced") {
            return Err(UnitError::new(UnitCode::Host, format!("podnet pod: the seccomp hold is {level}")));
        }
        self.capability.egress_hold = EgressHold::Advisory;
        let proof = self.capability.boundary_proof.take().unwrap_or_default();
        self.capability.boundary_proof = Some(format!("{proof}; hold: podnet pod seccomp {level}"));
        self.podnet = Some(podnet.to_path_buf());
        Ok(self)
    }

    /// The egress socket of a unit that the driver holds. A held driver
    /// refuses a unit with no socket directory: it does not run unheld.
    pub(super) fn egress_of(&self, spec: &StartSpec) -> Result<Option<PathBuf>, UnitError> {
        if self.podnet.is_none() {
            return Ok(None);
        }
        match &spec.sockets {
            Some(dir) => Ok(Some(dir.join(EGRESS_SOCKET))),
            None => Err(UnitError::new(UnitCode::UnitStartFailed, "a held unit needs its egress socket")),
        }
    }

    /// The command of `podvm`, under `podnet pod` when `egress` is given.
    pub(super) fn command(&self, egress: Option<&Path>) -> Command {
        match (&self.podnet, egress) {
            (Some(podnet), Some(egress)) => {
                let mut c = Command::new(podnet);
                c.args(["pod", "--backend", "seccomp", "-x", &format!("http://unix:{}", egress.display()), "--"])
                    .arg(&self.podvm.binary);
                c
            }
            _ => Command::new(&self.podvm.binary),
        }
    }
}

/// `hold.seccomp.level` of `podnet pod doctor --json`.
fn seccomp_level(out: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(out).ok()?;
    v["hold"]["seccomp"]["level"].as_str().or_else(|| v["seccomp"]["level"].as_str()).map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_level_comes_from_pod_doctor() {
        let doctor = br#"{"netns":{"level":"none"},"seccomp":{"level":"advisory","not_held":["unix sockets"]}}"#;
        assert_eq!(seccomp_level(doctor).as_deref(), Some("advisory"));
        let nested = br#"{"hold":{"seccomp":{"level":"enforced"}}}"#;
        assert_eq!(seccomp_level(nested).as_deref(), Some("enforced"));
        assert_eq!(seccomp_level(b"no json"), None);
    }
}
