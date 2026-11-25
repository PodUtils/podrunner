//! The confine layer of podvm (T-079, podvm `v0.2.0-beta.5`): with
//! `confine`, each `run -d` takes `--podvm-confine`, which applies the
//! Landlock rules, the seccomp deny filter of `pod-confine` and
//! `no_new_privs` to the payload. The record says `boundary: confined`
//! only after [`PodvmUnit::verify_confine`] on this host: an unconfined
//! unit reads the environment of a host process of the uid through
//! `/proc`, and a confined one writes its working directory and does not
//! read it (`scripts/measure-podvm-confine.sh`). The record lists the mount
//! mode `ro` only when the confined run also read a `:ro` host volume and
//! its write there did not reach the host (the cache of T-480).

use std::path::{Path, PathBuf};
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
/// The variable that a unit of the self-test reads in its own environment.
const MARK: &str = "PODRUNNER_CONFINE_SELF";
const MARK_VALUE: &str = "podrunner-confine-self";
/// The `:ro` volume of the confined run, its file and the file of its write.
const RO_TARGET: &str = "/podrunner-confine-ro";
const RO_FILE: &str = "mark";
const RO_VALUE: &str = "podrunner-confine-ro-read";
const RO_WRITE: &str = "write";

impl PodvmUnit {
    /// Give each unit the confine layer of podvm. The record changes only
    /// after [`PodvmUnit::verify_confine`].
    pub fn with_confine(mut self) -> PodvmUnit {
        self.confine = true;
        self.no_tier = true;
        self
    }

    /// The arguments of `run` for the confine layer, for a unit of the
    /// extracted image `rootfs`: read in the whole image, write and execute
    /// in the working directory, the runner directory and `/tmp` (each of
    /// the last two that the image has), and in the socket directory `sockets` when it
    /// is mounted (the hook relay and its sockets). The system directories
    /// of the image are read and execute by default. A host directory of
    /// `mounts` (the cache, T-480) is in `exec=`, and in `write=` unless it
    /// is `:ro`: a browser of Playwright runs from a cache.
    pub(super) fn confine_args(
        &self,
        rootfs: &Path,
        sockets: Option<&str>,
        mounts: &[(PathBuf, String)],
    ) -> Vec<String> {
        if !self.confine {
            return Vec::new();
        }
        // podvm makes the working directory of `-w` in the tree of the
        // unit; the image need not have it.
        let mut own: Vec<&str> = [WORKDIR, self.runner_dir.as_str(), "/tmp"]
            .into_iter()
            .filter(|p| *p == WORKDIR || rootfs.join(p.trim_start_matches('/')).is_dir())
            .collect();
        own.extend(sockets);
        let read_only = mounts.iter().filter_map(|(_, t)| t.strip_suffix(":ro"));
        let mut exec = own.clone();
        own.extend(mounts.iter().map(|(_, t)| t.as_str()).filter(|t| !t.ends_with(":ro")));
        exec.extend(own.iter().skip(exec.len()).copied().chain(read_only));
        let (own, exec) = (own.join(","), exec.join(","));
        vec!["--podvm-confine".into(), format!("read=/;write={own};exec={exec}")]
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
        self.capability.mount_modes = vec!["rw".into()];
        if !self.confine {
            return Ok(false);
        }
        // A rung with volumes binds the cache: the confined run proves a
        // `:ro` volume read-only.
        // A volume that the host cannot make leaves `ro` out, not the proof
        // of the layer.
        let ro = self.volumes.then(|| self.state_dir.join("confine-ro")).filter(|dir| {
            let _ = std::fs::remove_dir_all(dir);
            let made = std::fs::create_dir_all(dir).and_then(|()| std::fs::write(dir.join(RO_FILE), RO_VALUE));
            if let Err(e) = &made {
                eprintln!("podrunner: the ro volume of the confine self-test ({}): {e}: no ro mount", dir.display());
            }
            made.is_ok()
        });
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
            let open = self.self_test(reference, &unit, pid, None)?;
            self.confine = true;
            let held = self.self_test(reference, &unit, pid, None)?;
            let verdict = verdict(&open, &held, &canary);
            // A third run, so that a rung that refuses the volume, or a
            // failure of this run, keeps the proof of the layer.
            let ro_held = match ro.as_deref() {
                Some(d) if verdict.is_ok() => self
                    .self_test(reference, &unit, pid, Some(d))
                    .is_ok_and(|run| ro_verdict(&run, d.join(RO_WRITE).exists())),
                _ => false,
            };
            Ok::<_, UnitError>((verdict, ro_held))
        })();
        self.confine = true;
        let _ = victim.kill();
        let _ = victim.wait();
        if let Some(dir) = &ro {
            let _ = std::fs::remove_dir_all(dir);
        }
        let (verdict, ro_held) = result?;
        // Why the layer is not proven, for the line of `serve`.
        self.confine_why = verdict.as_ref().err().cloned();
        let verified = verdict.is_ok();
        if let Ok(by) = verdict {
            self.capability.boundary = Boundary::Confined;
            let proof = self.capability.boundary_proof.take().unwrap_or_default();
            let how = match by {
                Proof::Layer => "",
                Proof::Namespaces => " (the namespaces of the unit hide the processes of the host)",
            };
            self.capability.boundary_proof = Some(format!("{proof}; podvm --podvm-confine, verified{how}"));
            if ro_held {
                self.capability.mount_modes = vec!["ro".into(), "rw".into()];
            }
        }
        Ok(verified)
    }

    /// One run of the self-test: whether the payload wrote its own file,
    /// and what it read of the environment of `pid` (and of the `:ro`
    /// volume of the host directory `ro`, which it also tries to write).
    fn self_test(&self, reference: &str, unit: &UnitId, pid: u32, ro: Option<&Path>) -> Result<Run, UnitError> {
        let image = self.prepare(reference, &self.platform.clone(), false)?;
        // A unit of the last self-test, and the egress socket of a held
        // driver (T-183).
        let _ = self.destroy(unit);
        let held = self.self_test_sockets(unit)?;
        // The marker of the unit itself: a run that reads the environ of
        // its own shell (`/proc/$$`; `/proc/self` of a unit of the namespace
        // rung names another process) and not the canary is held by its
        // namespaces (T-566), not blind.
        let ro_step = if ro.is_some() {
            format!(
                "cat {RO_TARGET}/{RO_FILE} >> {SEEN} 2>/dev/null; echo >> {SEEN}; echo w > {RO_TARGET}/{RO_WRITE} 2>/dev/null; "
            )
        } else {
            String::new()
        };
        let script = format!(
            "echo own > {OWN}; tr '\\0' '\\n' < /proc/{pid}/environ > {SEEN} 2>/dev/null; \
             tr '\\0' '\\n' < /proc/$$/environ 2>/dev/null | grep '^{MARK}=' >> {SEEN}; {ro_step}echo done >> {SEEN}; exec sleep 60"
        );
        let spec = StartSpec {
            unit: unit.clone(),
            image,
            argv: ["sh", "-c", &script].map(String::from).to_vec(),
            env: [(MARK.to_string(), MARK_VALUE.to_string())].into(),
            credential: Default::default(),
            sockets: held.as_ref().map(|h| h.dir.clone()),
            mounts: ro.map(|d| vec![(d.to_path_buf(), format!("{RO_TARGET}:ro"))]).unwrap_or_default(),
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

/// Whether the confined run `held` read the `:ro` volume, so that it was
/// there, and its write did not reach the host (`wrote`).
fn ro_verdict(held: &Run, wrote: bool) -> bool {
    matches!(held, Ok((true, seen)) if seen.contains(RO_VALUE)) && !wrote
}

/// What proves that a unit cannot read a process of the host.
#[derive(Debug, PartialEq)]
enum Proof {
    /// The open run read the canary, the run with the layer did not.
    Layer,
    /// Neither run read it, and the open run read its own `/proc`: the
    /// PID namespace of podvm's namespace rung, or the user namespace of
    /// the netns hold, hides the host (T-566).
    Namespaces,
}

/// The verdict of the self-test: the open run read the canary, or read
/// its own `/proc/PID` and not the canary (the check can see it, or the
/// namespaces hold), and the held run wrote its own file and did not read
/// the canary. Else the first fact that fails, as one line.
fn verdict(open: &Run, held: &Run, canary: &str) -> Result<Proof, String> {
    let proof = match open {
        Err(why) => return Err(format!("the run with no layer: {why}")),
        Ok((_, seen)) if seen.contains(canary) => Proof::Layer,
        Ok((_, seen)) if seen.contains(&format!("{MARK}={MARK_VALUE}")) => Proof::Namespaces,
        Ok(_) => {
            return Err("the run with no layer reads neither a process of the host nor its own /proc, so the check proves nothing".into());
        }
    };
    match held {
        Err(why) => Err(format!("the run with the layer: {why}")),
        Ok((false, _)) => Err("the layer denies the unit the write of its own working directory".into()),
        Ok((true, seen)) if seen.contains(canary) => {
            Err("the run with the layer read the canary: the layer does not hold".into())
        }
        Ok(_) => Ok(proof),
    }
}

#[cfg(test)]
mod tests {
    use super::{MARK, MARK_VALUE, Proof, RO_VALUE, ro_verdict, verdict};

    /// P2-C of the second review of T-480: `ro` is in the record only when
    /// the confined run read the volume and did not write the host.
    #[test]
    fn a_ro_mount_is_proven_only_by_a_read_and_a_refused_write() {
        let read = Ok((true, format!("{RO_VALUE}\ndone\n")));
        assert!(ro_verdict(&read, false));
        // The write reached the host: the layer does not hold `:ro`.
        assert!(!ro_verdict(&read, true));
        // No volume to read: a failed write proves nothing.
        assert!(!ro_verdict(&Ok((true, "done\n".into())), false));
        assert!(!ro_verdict(&Err("the unit did not start".into()), false));
    }

    #[test]
    fn confine_is_proven_only_by_a_check_that_sees_and_a_layer_that_holds() {
        let c = "canary-1";
        let seen = Ok((true, format!("PODRUNNER_CONFINE_CANARY={c}\n{MARK}={MARK_VALUE}\ndone\n")));
        let blind = Ok((true, "done\n".to_string()));
        let own = Ok((true, format!("{MARK}={MARK_VALUE}\ndone\n")));
        assert_eq!(verdict(&seen, &blind, c), Ok(Proof::Layer));
        // T-566: the open run reads its own /proc and not the host (a PID
        // or user namespace): the namespaces hold, with the layer too.
        assert_eq!(verdict(&own, &blind, c), Ok(Proof::Namespaces));
        assert_eq!(verdict(&own, &own, c), Ok(Proof::Namespaces));
        // A plant: the open run sees nothing, also not itself (a rung with
        // no /proc): no proof, whatever the held run gives.
        assert!(verdict(&blind, &blind, c).unwrap_err().contains("proves nothing"));
        // A plant: the layer is not applied, the held run reads the canary.
        assert!(verdict(&seen, &seen, c).unwrap_err().contains("read the canary"));
        // The layer denies the own write too: no unit can work under it.
        assert!(verdict(&seen, &Ok((false, "done\n".into())), c).unwrap_err().contains("own working directory"));
        assert!(verdict(&own, &Ok((false, "done\n".into())), c).unwrap_err().contains("own working directory"));
        // A run that did not start proves nothing, and says why.
        let none = Err("the unit did not start: podvm run: exit 125".to_string());
        assert!(verdict(&none, &blind, c).unwrap_err().starts_with("the run with no layer: the unit did not start"));
        assert!(verdict(&seen, &none, c).unwrap_err().starts_with("the run with the layer: the unit did not start"));
    }
}
