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
use podrunner_unit::{EGRESS_SOCKET, StartSpec, UnitCode, UnitError, UnitId};
use serde_json::Value;

use super::{PodvmUnit, host};

impl PodvmUnit {
    /// Hold the egress of each unit with `podnet` (T-065, T-041). Refused
    /// when `podnet pod doctor --json` gives no hold of at least
    /// `advisory` on this host.
    pub fn with_podnet(self, podnet: &Path) -> Result<PodvmUnit, UnitError> {
        self.held(podnet, false)
    }

    /// [`Self::with_podnet`] with the `seccomp` backend also where `netns`
    /// holds: the local mode serves its broker on the loopback of the host,
    /// which a unit in a network namespace cannot reach (T-095, the QA
    /// finding A2 of T-246).
    pub fn with_podnet_seccomp(self, podnet: &Path) -> Result<PodvmUnit, UnitError> {
        self.held(podnet, true)
    }

    fn held(mut self, podnet: &Path, seccomp: bool) -> Result<PodvmUnit, UnitError> {
        let out = Command::new(podnet)
            .args(["pod", "doctor", "--json"])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| host("podnet pod doctor", e))?;
        let (backend, hold, level) = choose(&out.stdout, seccomp).map_err(|why| UnitError::new(UnitCode::Host, why))?;
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
            Some(dir) => {
                let egress = dir.join(EGRESS_SOCKET);
                // The port of the broker of a local run: the only loopback
                // port that the seccomp hold lets through (T-319).
                if let Some(port) = spec.env.get(podrunner_unit::LOOPBACK_ALLOW).and_then(|p| p.parse::<u16>().ok()) {
                    LOOPBACK.lock().unwrap_or_else(|e| e.into_inner()).insert(egress.clone(), port);
                }
                Ok(Some(egress))
            }
            None => Err(UnitError::new(UnitCode::UnitStartFailed, "a held unit needs its egress socket")),
        }
    }

    /// The socket directory of a self-test unit when the driver holds its
    /// units (T-183): a listener on its egress socket for the time of the
    /// test. The payload of a self-test makes no connection.
    pub(super) fn self_test_sockets(&self, unit: &UnitId) -> Result<Option<SelfTestSockets>, UnitError> {
        use std::os::unix::fs::PermissionsExt;
        if self.podnet.is_none() {
            return Ok(None);
        }
        let dir = self.state_dir.join("self-test").join(unit.as_str());
        let io = |e: std::io::Error| UnitError::new(UnitCode::Host, format!("the self-test sockets: {}", e.kind()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(io)?;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).map_err(io)?;
        let listener = std::os::unix::net::UnixListener::bind(dir.join(EGRESS_SOCKET)).map_err(io)?;
        Ok(Some(SelfTestSockets { dir, _listener: listener }))
    }

    /// The command of `podvm`, under `podnet pod` when `egress` is given.
    pub(super) fn command(&self, egress: Option<&Path>) -> Command {
        match (&self.podnet, egress) {
            (Some((podnet, backend)), Some(egress)) => {
                let mut c = Command::new(podnet);
                c.args(["pod", "--backend", backend, "-x", &format!("http://unix:{}", egress.display())]);
                let port = LOOPBACK.lock().unwrap_or_else(|e| e.into_inner()).get(egress).copied();
                if let (Some(port), "seccomp") = (port, *backend) {
                    c.args(loopback_rules(port));
                }
                c.arg("--").arg(&self.podvm.binary).env(crate::SPARE_TREE.0, crate::SPARE_TREE.1);
                c
            }
            _ => self.podvm.command(),
        }
    }
}

/// The port of the broker for each egress socket of a held unit (T-319).
static LOOPBACK: std::sync::Mutex<std::collections::BTreeMap<PathBuf, u16>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// The rules of `podnet pod --loopback-rules` (podnet `0.4.0-beta.10` and
/// later): the port of the broker goes direct, and each other port of the
/// loopback is refused, also a proxy there (PodUtils/podnet#605).
fn loopback_rules(port: u16) -> Vec<String> {
    let rules =
        [format!("tcp:127.0.0.1:{port}=direct"), "cidr:127.0.0.0/8=reject".into(), "cidr:::1/128=reject".into()];
    std::iter::once("--loopback-rules".to_string())
        .chain(rules.into_iter().flat_map(|r| ["-r".to_string(), r]))
        .collect()
}

/// The level of the backend `name` in `podnet pod doctor --json`.
fn level(v: &Value, name: &str) -> Option<String> {
    v["hold"][name]["level"].as_str().or_else(|| v[name]["level"].as_str()).map(String::from)
}

/// The backend and the hold from `podnet pod doctor --json`: `netns` only
/// when it says `enforced` and `seccomp` is not asked; `seccomp` is
/// `advisory` at most, whatever it says (it does not hold loopback TCP).
fn choose(out: &[u8], seccomp: bool) -> Result<(&'static str, EgressHold, String), String> {
    let v: Value = serde_json::from_slice(out).map_err(|_| "podnet pod doctor gives no JSON".to_string())?;
    if !seccomp && level(&v, "netns").as_deref() == Some("enforced") {
        return Ok(("netns", EgressHold::Enforced, "enforced".into()));
    }
    match level(&v, "seccomp") {
        Some(l) if l == "advisory" || l == "enforced" => Ok(("seccomp", EgressHold::Advisory, l)),
        Some(l) => Err(format!("podnet pod: the seccomp hold is {l}")),
        None => Err("podnet pod doctor gives no seccomp level".into()),
    }
}

/// The socket directory of a self-test unit; it goes when this goes.
pub(super) struct SelfTestSockets {
    pub(super) dir: PathBuf,
    _listener: std::os::unix::net::UnixListener,
}

impl Drop for SelfTestSockets {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hold_comes_from_pod_doctor() {
        let h0 = br#"{"netns":{"level":"none"},"seccomp":{"level":"advisory","not_held":["unix sockets"]}}"#;
        let pick = |o: &[u8], s| choose(o, s).map(|(b, h, _)| (b, h));
        assert_eq!(pick(h0, false).unwrap(), ("seccomp", EgressHold::Advisory));
        let ns = br#"{"hold":{"netns":{"level":"enforced"},"seccomp":{"level":"advisory"}}}"#;
        assert_eq!(pick(ns, false).unwrap(), ("netns", EgressHold::Enforced));
        // The local mode asks for seccomp also where netns holds.
        assert_eq!(pick(ns, true).unwrap(), ("seccomp", EgressHold::Advisory));
        // A plant: seccomp that claims `enforced` gives `advisory` only.
        let plant = br#"{"netns":{"level":"none"},"seccomp":{"level":"enforced"}}"#;
        assert_eq!(pick(plant, false).unwrap(), ("seccomp", EgressHold::Advisory));
        assert!(choose(br#"{"seccomp":{"level":"none"}}"#, false).is_err());
        assert!(choose(b"no json", false).is_err());
    }

    /// T-183: a held driver gives a self-test unit a socket directory with
    /// its egress socket, which goes at the end; an unheld driver none.
    #[test]
    fn a_held_self_test_has_its_egress_socket() {
        let _one = super::super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("podrunner-selftest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let script = |name: &str, body: &str| {
            let f = d.join(name);
            std::fs::write(&f, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
            f
        };
        let podvm = script("podvm", "echo '{\"podvm\":\"0.2.0-beta.21\",\"rung\":\"userland\"}'");
        let podnet = script("podnet", "echo '{\"seccomp\":{\"level\":\"advisory\"},\"netns\":{\"level\":\"none\"}}'");
        let unit = UnitId::parse("u-999999999").unwrap();
        let plain = PodvmUnit::new(crate::Podvm { binary: podvm.clone() }, &d.join("a"), &d).unwrap();
        assert!(plain.self_test_sockets(&unit).unwrap().is_none());
        let held =
            PodvmUnit::new(crate::Podvm { binary: podvm }, &d.join("b"), &d).unwrap().with_podnet(&podnet).unwrap();
        let s = held.self_test_sockets(&unit).unwrap().expect("a socket directory");
        let dir = s.dir.clone();
        assert!(dir.join(EGRESS_SOCKET).exists());
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        drop(s);
        assert!(!dir.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// T-319: the broker goes direct; each other loopback port is refused.
    #[test]
    fn the_rules_of_the_loopback() {
        assert_eq!(
            loopback_rules(4242),
            [
                "--loopback-rules",
                "-r",
                "tcp:127.0.0.1:4242=direct",
                "-r",
                "cidr:127.0.0.0/8=reject",
                "-r",
                "cidr:::1/128=reject"
            ]
        );
    }
}
