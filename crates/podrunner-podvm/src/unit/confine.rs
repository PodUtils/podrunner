//! The confine layer of podvm (T-079, podvm `v0.2.0-beta.5`): with
//! `confine`, each `run -d` takes `--podvm-confine`, which applies the
//! Landlock rules, the seccomp deny filter of `pod-confine` and
//! `no_new_privs` to the payload. The record says `boundary: confined`
//! only after [`PodvmUnit::verify_confine`] on this host: an unconfined
//! unit reads the environment of a host process of the uid through
//! `/proc`, and a confined one writes its working directory and does not
//! read it (`scripts/measure-podvm-confine.sh`).

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use podrunner_schema::{Boundary, UnitId};
use podrunner_unit::{StartSpec, UnitDriver, UnitError};

use super::{PodvmUnit, WORKDIR};

/// The file of the self-test that the payload writes in its working
/// directory: the environment of the canary, when it can read it.
const SEEN: &str = "podrunner-confine-seen";
/// The positive control of the self-test.
const OWN: &str = "podrunner-confine-own";

impl PodvmUnit {
    /// Give each unit the confine layer of podvm. The record changes only
    /// after [`PodvmUnit::verify_confine`].
    pub fn with_confine(mut self) -> PodvmUnit {
        self.confine = true;
        self
    }

    /// The arguments of `run` for the confine layer, for a unit whose
    /// rootfs is `rootfs`: read in the whole image, write and execute in
    /// the working directory, the runner directory and `/tmp` (each one
    /// that the image has), and in the socket directory `sockets` when it
    /// is mounted (the hook relay and its sockets). The system directories
    /// of the image are read and execute by default.
    pub(super) fn confine_args(&self, rootfs: &Path, sockets: Option<&str>) -> Vec<String> {
        if !self.confine {
            return Vec::new();
        }
        let mut own: Vec<&str> = [WORKDIR, self.runner_dir.as_str(), "/tmp"]
            .into_iter()
            .filter(|p| rootfs.join(p.trim_start_matches('/')).is_dir())
            .collect();
        own.extend(sockets);
        let own = own.join(",");
        vec!["--podvm-confine".into(), format!("read=/;write={own};exec={own}")]
    }

    /// The arguments of `run` for the confine layer of a container: read in
    /// the whole image, write in each writable mount, execute in each
    /// mount (the scripts of the steps, the externals of the runner).
    pub(super) fn container_confine_args(&self, mounts: &[podrunner_unit::ContainerMount]) -> Vec<String> {
        if !self.confine {
            return Vec::new();
        }
        let list = |ro: bool| {
            mounts.iter().filter(|m| ro || !m.read_only).map(|m| m.target.as_str()).collect::<Vec<_>>().join(",")
        };
        let mut spec = "read=/".to_string();
        for (key, paths) in [("write", list(false)), ("exec", list(true))] {
            if !paths.is_empty() {
                spec.push_str(&format!(";{key}={paths}"));
            }
        }
        vec!["--podvm-confine".into(), spec]
    }

    /// Prove the confine layer on this host with the pinned image
    /// `reference`. A host process of the uid holds a canary in its
    /// environment. The unit `unit` runs once with no layer, and must read
    /// the canary through `/proc` (else the check proves nothing), then
    /// once with the layer, and must write its working directory and not
    /// read the canary. Then the record says `boundary: confined`.
    pub fn verify_confine(&mut self, reference: &str, unit: UnitId) -> Result<bool, UnitError> {
        if !self.confine {
            return Ok(false);
        }
        let canary = format!("podrunner-confine-canary-{}", std::process::id());
        let mut victim = Command::new("sleep")
            .arg("120")
            .env_clear()
            .env("PODRUNNER_CONFINE_CANARY", &canary)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| super::host("the canary of the confine self-test", e))?;
        let pid = victim.id();
        let result = (|| {
            self.confine = false;
            let open = self.self_test(reference, &unit, pid)?;
            self.confine = true;
            let held = self.self_test(reference, &unit, pid)?;
            Ok::<_, UnitError>(proven(open.as_ref(), held.as_ref(), &canary))
        })();
        self.confine = true;
        let _ = victim.kill();
        let _ = victim.wait();
        let verified = result?;
        if verified {
            self.capability.boundary = Boundary::Confined;
            let proof = self.capability.boundary_proof.take().unwrap_or_default();
            self.capability.boundary_proof = Some(format!("{proof}; podvm --podvm-confine, verified"));
        }
        Ok(verified)
    }

    /// One run of the self-test: whether the payload wrote its own file,
    /// and what it read of the environment of `pid`.
    fn self_test(&self, reference: &str, unit: &UnitId, pid: u32) -> Result<Option<(bool, String)>, UnitError> {
        let image = self.prepare(reference, &self.platform.clone(), false)?;
        let script = format!(
            "echo own > {OWN}; tr '\\0' '\\n' < /proc/{pid}/environ > {SEEN} 2>/dev/null; echo done >> {SEEN}; exec sleep 60"
        );
        let spec = StartSpec {
            unit: unit.clone(),
            image,
            argv: ["sh", "-c", &script].map(String::from).to_vec(),
            env: Default::default(),
            credential: Default::default(),
            sockets: None,
            mounts: Vec::new(),
        };
        if self.start(&spec).is_err() {
            return Ok(None);
        }
        let work = self.observe(unit)?.unit_root;
        let end = Instant::now() + Duration::from_secs(20);
        let mut out = None;
        while let Some(w) = &work
            && Instant::now() < end
        {
            if let Ok(seen) = std::fs::read_to_string(w.join(SEEN))
                && seen.contains("done")
            {
                let own = std::fs::read_to_string(w.join(OWN)).is_ok_and(|t| t.trim() == "own");
                out = Some((own, seen));
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        self.destroy(unit)?;
        Ok(out)
    }
}

/// The verdict of the self-test: the open run read the canary (the check
/// can see it), and the held run wrote its own file and did not read it.
fn proven(open: Option<&(bool, String)>, held: Option<&(bool, String)>, canary: &str) -> bool {
    open.is_some_and(|(_, seen)| seen.contains(canary))
        && held.is_some_and(|(own, seen)| *own && !seen.contains(canary))
}

#[cfg(test)]
mod tests {
    use super::proven;

    #[test]
    fn confine_is_proven_only_by_a_check_that_sees_and_a_layer_that_holds() {
        let c = "canary-1";
        let seen = (true, format!("PODRUNNER_CONFINE_CANARY={c}\ndone\n"));
        let blind = (true, "done\n".to_string());
        assert!(proven(Some(&seen), Some(&blind), c));
        // A plant: the open run sees nothing (a rung with no /proc): no
        // proof, whatever the held run gives.
        assert!(!proven(Some(&blind), Some(&blind), c));
        // A plant: the layer is not applied, the held run reads the canary.
        assert!(!proven(Some(&seen), Some(&seen), c));
        // The layer denies the own write too: no unit can work under it.
        assert!(!proven(Some(&seen), Some(&(false, "done\n".into())), c));
        // A run that did not start proves nothing.
        assert!(!proven(None, Some(&blind), c) && !proven(Some(&seen), None, c));
    }
}
