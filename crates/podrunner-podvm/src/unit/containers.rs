//! The containers of a podvm unit (T-035, step 4): each container is a
//! podvm container in the store of its unit, with the volumes of the hook
//! tables as `-v`. The podvm verbs: `run -d` for a job container or a
//! service, `exec -e -w` for a step, `logs`, `inspect`, `rm -f`. Volumes
//! work on the supervise rung only (U19, T-034); the probe gives
//! `container: hooks` there. Each container carries the tag of its unit,
//! so the tree kill of `destroy` reaches it.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use podrunner_schema::UnitId;
use podrunner_unit::procs::TAG_VAR;
use podrunner_unit::{ContainerId, ContainerSpec, ContainerState, ExecSpec, Exit, UnitCode, UnitError, parse_pinned};
use serde_json::Value;

use super::{PodvmUnit, host};

/// Where a unit sees its socket directory (`StartSpec::sockets`).
pub(super) const SOCKET_MOUNT: &str = "/run/podrunner";

fn refused(why: &str) -> UnitError {
    UnitError::new(UnitCode::ContainerOptionRefused, why)
}

impl PodvmUnit {
    /// The store and the tag of `unit`, and the host path of its runner
    /// directory: a relative mount source is under it.
    fn container_base(&self, unit: &UnitId) -> Result<(PathBuf, String, PathBuf, Option<PathBuf>), UnitError> {
        let units = self.units.lock().expect("the units");
        let r = units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
        let runner = r.rootfs.join(self.runner_dir.trim_start_matches('/'));
        Ok((r.dir.join("store"), r.tag.clone(), runner, r.egress.clone()))
    }

    /// `podvm ARGS` in the store `store`, with its status and its output.
    /// `podvm ARGS` in the store `store`, under the hold of `egress` when
    /// it is given (T-065), with its status and its output.
    fn podvm_in(
        &self,
        store: &Path,
        args: &[String],
        tag: &str,
        egress: Option<&Path>,
    ) -> Result<(Option<i32>, Vec<u8>), UnitError> {
        let out = self
            .command(egress)
            .args(args)
            .env("PODVM_STORE", store)
            .env(TAG_VAR, tag)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| host("podvm", e))?;
        Ok((out.status.code(), [out.stdout, out.stderr].concat()))
    }

    pub(super) fn start_container(&self, unit: &UnitId, spec: &ContainerSpec) -> Result<ContainerId, UnitError> {
        if self.capability.container != podrunner_schema::ContainerRoute::Hooks {
            return Err(UnitError::new(UnitCode::ContainerNotSupported, "volumes need the supervise rung of podvm"));
        }
        parse_pinned(&spec.image)?;
        if !spec.options.is_empty() {
            return Err(refused("the podvm unit applies no create option"));
        }
        let prepared = podrunner_unit::UnitDriver::prepare(self, &spec.image, &self.platform.clone(), false)?;
        let (store, tag, runner, egress) = self.container_base(unit)?;
        let tar = self.tar_of(&prepared.digest);
        let (code, out) =
            self.podvm_in(&store, &["load".into(), "-i".into(), tar.display().to_string()], &tag, None)?;
        if code != Some(0) {
            return Err(UnitError::new(UnitCode::UnitStartFailed, format!("podvm load: {}", last_line(&out))));
        }
        let n = self.containers.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!("{unit}-c{n}");
        let mut args: Vec<String> = ["run", "-d", "--name", &name].map(String::from).to_vec();
        if let Some(w) = &spec.workdir {
            args.extend(["-w".into(), w.clone()]);
        }
        args.extend(["-e".into(), format!("{TAG_VAR}={tag}")]);
        for (k, v) in &spec.env {
            args.extend(["-e".into(), format!("{k}={v}")]);
        }
        for m in &spec.mounts {
            // A relative source is under the runner directory of the unit.
            let source = if m.source.is_absolute() { m.source.clone() } else { runner.join(&m.source) };
            // podvm needs the source; docker makes a missing one. Only a
            // directory of the root of this unit is made here.
            if !source.exists() && !m.source.is_absolute() {
                std::fs::create_dir_all(&source).map_err(|e| host("a volume source", e))?;
            }
            let ro = if m.read_only { ":ro" } else { "" };
            args.extend(["-v".into(), format!("{}:{}{ro}", source.display(), m.target)]);
        }
        args.push(spec.image.clone());
        args.extend(spec.argv.iter().cloned());
        let (code, out) = self.podvm_in(&store, &args, &tag, egress.as_deref())?;
        if code != Some(0) {
            return Err(UnitError::new(UnitCode::UnitStartFailed, format!("podvm run: {}", last_line(&out))));
        }
        Ok(ContainerId(name))
    }

    pub(super) fn exec_container(&self, unit: &UnitId, c: &ContainerId, spec: &ExecSpec) -> Result<Exit, UnitError> {
        if spec.stdin {
            return Err(refused("stdin"));
        }
        let (store, tag, _, egress) = self.container_base(unit)?;
        let mut args: Vec<String> = vec!["exec".into()];
        if let Some(w) = &spec.workdir {
            args.extend(["-w".into(), w.clone()]);
        }
        for (k, v) in &spec.env {
            args.extend(["-e".into(), format!("{k}={v}")]);
        }
        args.push(c.0.clone());
        args.extend(spec.argv.iter().cloned());
        let (code, out) = self.podvm_in(&store, &args, &tag, egress.as_deref())?;
        if let Some(log) = &spec.log {
            std::fs::write(log, &out).map_err(|e| host("the log of a step", e))?;
        }
        Ok(Exit::Code(code.unwrap_or(-1)))
    }

    pub(super) fn logs_container(&self, unit: &UnitId, c: &ContainerId) -> Result<Vec<u8>, UnitError> {
        let (store, tag, _, _) = self.container_base(unit)?;
        let (code, out) = self.podvm_in(&store, &["logs".into(), c.0.clone()], &tag, None)?;
        if code != Some(0) {
            return Err(UnitError::new(UnitCode::UnknownUnit, c.0.clone()));
        }
        Ok(out)
    }

    pub(super) fn inspect_container(&self, unit: &UnitId, c: &ContainerId) -> Result<ContainerState, UnitError> {
        let (store, tag, _, _) = self.container_base(unit)?;
        let (code, out) = self.podvm_in(&store, &["inspect".into(), c.0.clone()], &tag, None)?;
        if code != Some(0) {
            return Err(UnitError::new(UnitCode::UnknownUnit, c.0.clone()));
        }
        state_of(&out)
    }

    pub(super) fn stop_container(&self, unit: &UnitId, c: &ContainerId) -> Result<(), UnitError> {
        let (store, tag, _, _) = self.container_base(unit)?;
        self.podvm_in(&store, &["rm".into(), "-f".into(), c.0.clone()], &tag, None).map(drop)
    }
}

fn last_line(out: &[u8]) -> String {
    String::from_utf8_lossy(out).lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").chars().take(200).collect()
}

/// The state in the output of `podvm inspect`: `State` and `ExitCode`.
fn state_of(out: &[u8]) -> Result<ContainerState, UnitError> {
    let text = String::from_utf8_lossy(out);
    let start = text.find('[').ok_or_else(|| UnitError::new(UnitCode::Host, "podvm inspect gave no JSON"))?;
    let v: Value = serde_json::from_str(&text[start..])
        .map_err(|_| UnitError::new(UnitCode::Host, "podvm inspect gave no JSON"))?;
    let c = &v[0];
    let running = c["State"].as_str() == Some("running");
    let exit =
        c["ExitCode"].as_str().and_then(|s| s.parse::<i32>().ok()).or_else(|| c["ExitCode"].as_i64().map(|n| n as i32));
    Ok(ContainerState { running, exit: if running { None } else { exit.map(Exit::Code) } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_comes_from_inspect() {
        let up = br#"x: Extracted
[{"Name":"u-1-c0","State":"running","ExitCode":"-"}]"#;
        assert_eq!(state_of(up).unwrap(), ContainerState { running: true, exit: None });
        let done = br#"[{"Name":"u-1-c0","State":"exited","ExitCode":"3"}]"#;
        assert_eq!(state_of(done).unwrap(), ContainerState { running: false, exit: Some(Exit::Code(3)) });
        assert!(state_of(b"no json").is_err());
    }
}
