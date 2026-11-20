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
        self.no_tier = true;
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

    /// Why the last [`PodvmUnit::verify_confine`] did not prove the layer.
    pub fn confine_why(&self) -> Option<&str> {
        self.confine_why.as_deref()
    }

    /// Prove the confine layer on this host with the pinned image
    /// `reference`. A host process of the uid holds a canary in its
    /// environment. The unit `unit` runs once with no layer, and must read
    /// the canary through `/proc` (else the check proves nothing), then
    /// once with the layer, and must write its working directory and not
    /// read the canary. Then the record says `boundary: confined`.
    pub fn verify_confine(&mut self, reference: &str, unit: UnitId) -> Result<bool, UnitError> {
        self.confine_why = None;
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
            Ok::<_, UnitError>(verdict(&open, &held, &canary))
        })();
        self.confine = true;
        let _ = victim.kill();
        let _ = victim.wait();
        let verdict = result?;
        // Why the layer is not proven, for the line of `serve`.
        self.confine_why = verdict.as_ref().err().cloned();
        let verified = verdict.is_ok();
        if verified {
            self.capability.boundary = Boundary::Confined;
            let proof = self.capability.boundary_proof.take().unwrap_or_default();
            self.capability.boundary_proof = Some(format!("{proof}; podvm --podvm-confine, verified"));
        }
        Ok(verified)
    }

    /// One run of the self-test: whether the payload wrote its own file,
    /// and what it read of the environment of `pid`.
    fn self_test(&self, reference: &str, unit: &UnitId, pid: u32) -> Result<Run, UnitError> {
        let image = self.prepare(reference, &self.platform.clone(), false)?;
        // A unit of the last self-test, and the egress socket of a held
        // driver (T-183).
        let _ = self.destroy(unit);
        let held = self.self_test_sockets(unit)?;
        let script = format!(
            "echo own > {OWN}; tr '\\0' '\\n' < /proc/{pid}/environ > {SEEN} 2>/dev/null; echo done >> {SEEN}; exec sleep 60"
        );
        let spec = StartSpec {
            unit: unit.clone(),
            image,
            argv: ["sh", "-c", &script].map(String::from).to_vec(),
            env: Default::default(),
            credential: Default::default(),
            sockets: held.as_ref().map(|h| h.dir.clone()),
            mounts: Vec::new(),
            limits: Default::default(),
        };
        if let Err(e) = self.start(&spec) {
            // One line: the cause goes into a line of `serve`.
            let detail = e.detail.split_whitespace().collect::<Vec<_>>().join(" ");
            return Ok(Err(format!("the unit did not start: {detail}")));
        }
        let work = self.observe(unit)?.unit_root;
        let end = Instant::now() + Duration::from_secs(20);
        let mut out = Err("the unit wrote no result in 20 s".to_string());
        while let Some(w) = &work
            && Instant::now() < end
        {
            if let Ok(seen) = std::fs::read_to_string(w.join(SEEN))
                && seen.contains("done")
            {
                let own = std::fs::read_to_string(w.join(OWN)).is_ok_and(|t| t.trim() == "own");
                out = Ok((own, seen));
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        self.destroy(unit)?;
        Ok(out)
    }
}

/// One run of the self-test: whether it wrote its own file and what it
/// read of the canary, else why it gave no result.
type Run = Result<(bool, String), String>;

/// The verdict of the self-test: the open run read the canary (the check
/// can see it), and the held run wrote its own file and did not read it.
/// Else the first fact that fails, as one line.
fn verdict(open: &Run, held: &Run, canary: &str) -> Result<(), String> {
    match open {
        Err(why) => return Err(format!("the run with no layer: {why}")),
        Ok((_, seen)) if !seen.contains(canary) => {
            return Err("the run with no layer cannot read a process of the host through /proc (a PID namespace of podvm's namespace rung, or the user namespace of the netns hold), so the check proves nothing".into());
        }
        Ok(_) => {}
    }
    match held {
        Err(why) => Err(format!("the run with the layer: {why}")),
        Ok((false, _)) => Err("the layer denies the unit the write of its own working directory".into()),
        Ok((true, seen)) if seen.contains(canary) => {
            Err("the run with the layer read the canary: the layer does not hold".into())
        }
        Ok(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::verdict;

    #[test]
    fn confine_is_proven_only_by_a_check_that_sees_and_a_layer_that_holds() {
        let c = "canary-1";
        let seen = Ok((true, format!("PODRUNNER_CONFINE_CANARY={c}\ndone\n")));
        let blind = Ok((true, "done\n".to_string()));
        assert_eq!(verdict(&seen, &blind, c), Ok(()));
        // A plant: the open run sees nothing (a rung with no /proc of the
        // host): no proof, whatever the held run gives (T-566).
        assert!(verdict(&blind, &blind, c).unwrap_err().contains("PID namespace"));
        // A plant: the layer is not applied, the held run reads the canary.
        assert!(verdict(&seen, &seen, c).unwrap_err().contains("read the canary"));
        // The layer denies the own write too: no unit can work under it.
        assert!(verdict(&seen, &Ok((false, "done\n".into())), c).unwrap_err().contains("own working directory"));
        // A run that did not start proves nothing, and says why.
        let none = Err("the unit did not start: podvm run: exit 125".to_string());
        assert!(verdict(&none, &blind, c).unwrap_err().starts_with("the run with no layer: the unit did not start"));
        assert!(verdict(&seen, &none, c).unwrap_err().starts_with("the run with the layer: the unit did not start"));
    }
}
