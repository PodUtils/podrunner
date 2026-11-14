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

    /// The reference that a unit store knows after the load of `tar`: the
    /// name of `image` with the digest of the manifest of the tar (T-130). An
    /// image pinned by the digest of an index loads as its platform manifest,
    /// so `podvm run` of the index digest finds no image.
    fn loaded_reference(image: &str, tar: &std::path::Path) -> String {
        let manifest = std::fs::File::open(tar).ok().and_then(|f| {
            let mut a = tar::Archive::new(f);
            let mut entry =
                a.entries().ok()?.flatten().find(|e| e.path().is_ok_and(|p| p.as_os_str() == "index.json"))?;
            let mut text = String::new();
            std::io::Read::read_to_string(&mut entry, &mut text).ok()?;
            let v: serde_json::Value = serde_json::from_str(&text).ok()?;
            v["manifests"][0]["digest"].as_str().map(str::to_string)
        });
        match (image.split_once('@'), manifest) {
            (Some((name, _)), Some(m)) => format!("{name}@{m}"),
            _ => image.to_string(),
        }
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
        args.extend(super::labels::label_args(unit.as_str()));
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
        args.extend(self.container_confine_args(&spec.mounts));
        args.push(Self::loaded_reference(&spec.image, &tar));
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

    /// `podvm logs` of the detached payload of `unit`.
    pub(super) fn logs_unit(&self, unit: &UnitId) -> Result<Vec<u8>, UnitError> {
        let (store, tag, _, _) = self.container_base(unit)?;
        let (code, out) = self.podvm_in(&store, &["logs".into(), unit.to_string()], &tag, None)?;
        if code != Some(0) {
            return Err(UnitError::new(UnitCode::UnknownUnit, unit.to_string()));
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

/// The last line of an output, cut to `LINE` characters with a mark (no
/// credential is in the output of podvm: the credential goes in files).
/// A refusal of podvm gives its reason at the end of a long line.
pub(super) fn last_line(out: &[u8]) -> String {
    const LINE: usize = 600;
    let text = String::from_utf8_lossy(out);
    let line = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
    if line.chars().count() <= LINE {
        return line.to_string();
    }
    let mut cut: String = line.chars().take(LINE).collect();
    cut.push_str(" [cut]");
    cut
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

    /// A refusal of podvm keeps its reason; a longer line shows its cut.
    #[test]
    fn the_last_line_keeps_the_reason_of_podvm() {
        let refusal = "podvm run: chroot(2) is denied on this machine, and no no-chroot family runs this payload: \
                       the loader family declines the payload: this podvm binary carries no musl interposer object, \
                       so the tier is declined; use a podvm build with the musl interposer or a host that allows chroot";
        let out = format!("Extracted\n{refusal}\n\n");
        assert_eq!(last_line(out.as_bytes()), refusal);
        let long = "x".repeat(5000);
        let cut = last_line(long.as_bytes());
        assert!(cut.ends_with(" [cut]") && cut.len() < 1000, "{}", cut.len());
    }
}

#[cfg(test)]
mod loaded_tests {
    use super::*;

    #[test]
    fn an_image_of_an_index_runs_by_the_digest_of_its_loaded_manifest() {
        let dir = std::env::temp_dir().join(format!("podrunner-loaded-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tar_path = dir.join("i.tar");
        let index = format!(r#"{{"schemaVersion":2,"manifests":[{{"digest":"sha256:{}"}}]}}"#, "b".repeat(64));
        let mut b = tar::Builder::new(std::fs::File::create(&tar_path).unwrap());
        let mut h = tar::Header::new_gnu();
        h.set_size(index.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        b.append_data(&mut h, "index.json", index.as_bytes()).unwrap();
        b.finish().unwrap();
        let index_digest = format!("busybox@sha256:{}", "a".repeat(64));
        assert_eq!(PodvmUnit::loaded_reference(&index_digest, &tar_path), format!("busybox@sha256:{}", "b".repeat(64)));
        assert_eq!(
            PodvmUnit::loaded_reference(&index_digest, &dir.join("none.tar")),
            index_digest,
            "no tar: the reference stays"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
