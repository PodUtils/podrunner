//! The egress hold of a podvm unit (T-065, T-041): `podvm run -d` of the
//! unit, `run -d` of each container and each `exec` run under `podnet pod
//! -x http://unix:EGRESS`, so that each TCP connect of the unit goes to its
//! egress socket. The payload is held for its whole life after `podnet
//! pod` returns; an `exec` is held only under a `podnet pod` of its own
//! (`scripts/measure-podnet-pod.sh`).
//!
//! - `netns` (`enforced`): a new network namespace whose only route is the
//!   socket. It needs user namespaces and `/dev/net/tun`. A direct connect
//!   to the loopback of the host fails.
//! - `seccomp` (`advisory`): on a host with no user namespace. Loopback
//!   TCP and unix sockets are not held.
//!
//! The driver takes `netns` when `podnet pod doctor --json` says that it
//! is `enforced`, else `seccomp` when it is at least `advisory`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use podrunner_schema::EgressHold;
use podrunner_unit::{EGRESS_SOCKET, StartSpec, UnitCode, UnitError};
use serde_json::Value;

use super::{PodvmUnit, host};

impl PodvmUnit {
    /// Hold the egress of each unit with `podnet` (T-065, T-041). Refused
    /// when `podnet pod doctor --json` gives no hold of at least
    /// `advisory` on this host.
    pub fn with_podnet(mut self, podnet: &Path) -> Result<PodvmUnit, UnitError> {
        let out = Command::new(podnet)
            .args(["pod", "doctor", "--json"])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| host("podnet pod doctor", e))?;
        let (backend, hold, level) = choose(&out.stdout).map_err(|why| UnitError::new(UnitCode::Host, why))?;
        self.capability.egress_hold = hold;
        let proof = self.capability.boundary_proof.take().unwrap_or_default();
        self.capability.boundary_proof = Some(format!("{proof}; hold: podnet pod {backend} {level}"));
        self.podnet = Some((podnet.to_path_buf(), backend));
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
            (Some((podnet, backend)), Some(egress)) => {
                let mut c = Command::new(podnet);
                c.args(["pod", "--backend", backend, "-x", &format!("http://unix:{}", egress.display()), "--"])
                    .arg(&self.podvm.binary);
                c
            }
            _ => Command::new(&self.podvm.binary),
        }
    }
}

/// The level of the backend `name` in `podnet pod doctor --json`.
fn level(v: &Value, name: &str) -> Option<String> {
    v["hold"][name]["level"].as_str().or_else(|| v[name]["level"].as_str()).map(String::from)
}

/// The backend and the hold from `podnet pod doctor --json`: `netns` only
/// when it says `enforced`; `seccomp` is `advisory` at most, whatever it
/// says (it does not hold loopback TCP).
fn choose(out: &[u8]) -> Result<(&'static str, EgressHold, String), String> {
    let v: Value = serde_json::from_slice(out).map_err(|_| "podnet pod doctor gives no JSON".to_string())?;
    if level(&v, "netns").as_deref() == Some("enforced") {
        return Ok(("netns", EgressHold::Enforced, "enforced".into()));
    }
    match level(&v, "seccomp") {
        Some(l) if l == "advisory" || l == "enforced" => Ok(("seccomp", EgressHold::Advisory, l)),
        Some(l) => Err(format!("podnet pod: the seccomp hold is {l}")),
        None => Err("podnet pod doctor gives no seccomp level".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hold_comes_from_pod_doctor() {
        let h0 = br#"{"netns":{"level":"none"},"seccomp":{"level":"advisory","not_held":["unix sockets"]}}"#;
        assert_eq!((choose(h0).unwrap().0, choose(h0).unwrap().1), ("seccomp", EgressHold::Advisory));
        let ns = br#"{"hold":{"netns":{"level":"enforced"},"seccomp":{"level":"advisory"}}}"#;
        assert_eq!((choose(ns).unwrap().0, choose(ns).unwrap().1), ("netns", EgressHold::Enforced));
        // A plant: seccomp that claims `enforced` gives `advisory` only.
        let plant = br#"{"netns":{"level":"none"},"seccomp":{"level":"enforced"}}"#;
        assert_eq!((choose(plant).unwrap().0, choose(plant).unwrap().1), ("seccomp", EgressHold::Advisory));
        assert!(choose(br#"{"seccomp":{"level":"none"}}"#).is_err());
        assert!(choose(b"no json").is_err());
    }
}
