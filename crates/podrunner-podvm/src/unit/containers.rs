//! The containers of a podvm unit (T-035, step 4): each container is a
//! podvm container in the store of its unit (T-545), with the volumes of the hook
//! tables as `-v`. The podvm verbs: `run -d` for a job container or a
//! service, `exec -e -w` for a step, `logs`, `inspect`, `rm -f`. Volumes
//! work in each step on the supervise and userland rungs (U19, T-034),
//! and on the namespace rung from podvm `0.2.0-beta.37`, where `exec`
//! joins the container (T-532); the probe gives `container: hooks` there.
//! On the namespace rung each `run -d` takes `--podvm-volume-base` of the
//! runner directory: podvm opens each source under it with no symbolic
//! link, so a link that a step makes after the check of the hooks cannot
//! redirect the bind (T-403); a source out of it (the CA directory of the
//! hooks, T-578) binds by its path. Each container
//! carries the tag of its unit, so the tree kill of `destroy` reaches it.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use podrunner_schema::UnitId;
use podrunner_unit::procs::TAG_VAR;
use podrunner_unit::{
    ContainerId, ContainerMount, ContainerSpec, ContainerState, ExecSpec, Exit, UnitCode, UnitError, parse_pinned,
};
use serde_json::Value;

use super::{PodvmUnit, host};

/// Where a unit sees its socket directory (`StartSpec::sockets`).
pub(super) const SOCKET_MOUNT: &str = "/run/podrunner";

fn refused(why: &str) -> UnitError {
    UnitError::new(UnitCode::ContainerOptionRefused, why)
}

/// What the verbs of a container of a unit need.
struct Base {
    /// The store of the unit.
    store: PathBuf,
    /// The directory of the unit: the environment files go there.
    dir: PathBuf,
    tag: String,
    /// The host path of the runner directory of the unit: a relative
    /// mount source is under it.
    runner: PathBuf,
    egress: Option<PathBuf>,
}

impl PodvmUnit {
    fn container_base(&self, unit: &UnitId) -> Result<Base, UnitError> {
        let units = self.units.lock().expect("the units");
        let r = units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
        let runner = r.rootfs.join(self.runner_dir.trim_start_matches('/'));
        Ok(Base { store: r.store.clone(), dir: r.dir.clone(), tag: r.tag.clone(), runner, egress: r.egress.clone() })
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
            return Err(UnitError::new(
                UnitCode::ContainerNotSupported,
                "the steps of a container need the supervise or userland rung of podvm, or its namespace rung from 0.2.0-beta.37",
            ));
        }
        parse_pinned(&spec.image)?;
        if !spec.options.is_empty() {
            return Err(refused("the podvm unit applies no create option"));
        }
        let prepared = podrunner_unit::UnitDriver::prepare(self, &spec.image, &self.platform.clone(), false)?;
        let Base { store, dir, tag, runner, egress } = self.container_base(unit)?;
        // The image is loaded once into the store of the units, and run by
        // the digest of its manifest (T-130, T-545).
        let reference = self.loaded(&store, &self.tar_of(&prepared.digest))?;
        let n = self.containers.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!("{unit}-c{n}");
        let mut args: Vec<String> = ["run", "-d", "--name", &name].map(String::from).to_vec();
        args.extend(super::labels::label_args(unit.as_str()));
        if let Some(w) = &spec.workdir {
            args.extend(["-w".into(), w.clone()]);
        }
        args.extend(["-e".into(), format!("{TAG_VAR}={tag}")]);
        args.extend(volume_base_args(crate::probe::rung(&self.capability), &runner));
        let (env_args, _env_file) = super::envfile::env_args(&self.podvm, &dir, &spec.env)?;
        args.extend(env_args);
        args.extend(volume_args(&runner, &spec.mounts)?);
        args.extend(self.container_confine_args(&spec.mounts));
        args.push(reference);
        args.extend(spec.argv.iter().cloned());
        let (code, out) = self.podvm_in(&store, &args, &tag, egress.as_deref())?;
        if code != Some(0) {
            return Err(run_error(code, &out));
        }
        Ok(ContainerId(name))
    }

    pub(super) fn exec_container(&self, unit: &UnitId, c: &ContainerId, spec: &ExecSpec) -> Result<Exit, UnitError> {
        if spec.stdin {
            return Err(refused("stdin"));
        }
        let Base { store, dir, tag, egress, .. } = self.container_base(unit)?;
        let mut args: Vec<String> = vec!["exec".into()];
        args.extend(super::labels::fixup_args(&self.podvm).iter().map(|a| a.to_string()));
        if let Some(w) = &spec.workdir {
            args.extend(["-w".into(), w.clone()]);
        }
        let (env_args, _env_file) = super::envfile::env_args(&self.podvm, &dir, &spec.env)?;
        args.extend(env_args);
        args.push(c.0.clone());
        args.extend(spec.argv.iter().cloned());
        let (code, out) = self.podvm_in(&store, &args, &tag, egress.as_deref())?;
        if let Some(log) = &spec.log {
            std::fs::write(log, &out).map_err(|e| host("the log of a step", e))?;
        }
        exec_exit(&c.0, code, &out)
    }

    pub(super) fn logs_container(&self, unit: &UnitId, c: &ContainerId) -> Result<Vec<u8>, UnitError> {
        let Base { store, tag, .. } = self.container_base(unit)?;
        let (code, out) = self.podvm_in(&store, &["logs".into(), c.0.clone()], &tag, None)?;
        if code != Some(0) {
            return Err(UnitError::new(UnitCode::UnknownUnit, c.0.clone()));
        }
        Ok(out)
    }

    /// `podvm logs` of the detached payload of `unit`.
    /// `podvm exec` in the unit, under its hold (T-150).
    pub(super) fn enter_unit(&self, unit: &UnitId, argv: &[String], tty: bool) -> Result<Command, UnitError> {
        let Base { store, tag, egress, .. } = self.container_base(unit)?;
        let mut c = self.command(egress.as_deref());
        c.args(["exec", "-i"]).args(super::labels::fixup_args(&self.podvm));
        if tty {
            c.arg("-t");
        }
        c.arg(unit.as_str()).args(argv).env("PODVM_STORE", store).env(TAG_VAR, tag);
        Ok(c)
    }

    /// The stdout of `podvm logs` as a stream (T-570): `podvm logs` writes
    /// the bytes of the payload to its stdout, and its own text to stderr.
    /// A bounded read gives its bound to podvm (`Podvm::logs`).
    pub(super) fn log_unit(&self, unit: &UnitId) -> Result<podrunner_unit::UnitLog, UnitError> {
        let Base { store, tag, .. } = self.container_base(unit)?;
        Ok(self.podvm.logs(&[], unit.as_str(), vec![("PODVM_STORE", store.into()), (TAG_VAR, tag.into())]))
    }

    pub(super) fn inspect_container(&self, unit: &UnitId, c: &ContainerId) -> Result<ContainerState, UnitError> {
        let Base { store, tag, .. } = self.container_base(unit)?;
        let (code, out) = self.podvm_in(&store, &["inspect".into(), c.0.clone()], &tag, None)?;
        if code != Some(0) {
            return Err(UnitError::new(UnitCode::UnknownUnit, c.0.clone()));
        }
        state_of(&out)
    }

    pub(super) fn stop_container(&self, unit: &UnitId, c: &ContainerId) -> Result<(), UnitError> {
        let Base { store, tag, .. } = self.container_base(unit)?;
        self.podvm_in(&store, &["rm".into(), "-f".into(), c.0.clone()], &tag, None).map(drop)
    }
}

/// `--podvm-volume-base RUNNER` on the namespace rung (podvm `0.2.0-beta.32`
/// and later; the containers run there from `0.2.0-beta.37`): each `-v`
/// source under the runner directory opens with no symbolic link (T-403,
/// T-532). The supervise rung refuses the flag.
fn volume_base_args(rung: Option<&str>, runner: &Path) -> Vec<String> {
    if rung == Some("namespace") {
        vec!["--podvm-volume-base".into(), runner.display().to_string()]
    } else {
        Vec::new()
    }
}

/// The `-v` of each mount of a container. A relative source is under the
/// runner directory of the unit, the volume base of the namespace rung.
/// An absolute source is a directory that the host made out of the reach
/// of the job (the CA bundle of the hooks, T-578): podvm binds a source
/// out of `--podvm-volume-base` by its path, as with no base (podvm
/// `0.2.0-beta.37`, `beneath` of `userns/host.rs`; measured on the
/// namespace rung).
fn volume_args(runner: &Path, mounts: &[ContainerMount]) -> Result<Vec<String>, UnitError> {
    let mut args = Vec::new();
    for m in mounts {
        let source = if m.source.is_absolute() { m.source.clone() } else { runner.join(&m.source) };
        if !m.source.is_absolute() {
            no_link(runner, &m.source)?;
        }
        // podvm needs the source; docker makes a missing one. Only a
        // directory of the root of this unit is made here.
        if !source.exists() && !m.source.is_absolute() {
            std::fs::create_dir_all(&source).map_err(|e| host("a volume source", e))?;
        }
        podrunner_unit::mounts::host_path(&source, UnitCode::ContainerOptionRefused)?;
        let ro = if m.read_only { ":ro" } else { "" };
        args.extend(["-v".into(), format!("{}:{}{ro}", source.display(), m.target)]);
    }
    Ok(args)
}

/// Refuse a relative mount source with a symbolic link under the runner
/// directory, before podrunner makes a missing part: the job writes that
/// tree (T-403). The hooks see no host path of a podvm unit, so the
/// driver checks; the volume base of podvm closes the race after this
/// check.
fn no_link(runner: &Path, rel: &Path) -> Result<(), UnitError> {
    let mut at = runner.to_path_buf();
    for part in rel.components() {
        at.push(part);
        if std::fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(refused(&format!("the mount source {} holds a symbolic link", rel.display())));
        }
    }
    Ok(())
}

/// The error of a `run -d` that did not start. A volume that podvm refuses
/// (exit 125: a symbolic link under `--podvm-volume-base`, made after the
/// check of podrunner) is a refused option (T-403); each other end is a
/// start that failed.
fn run_error(code: Option<i32>, out: &[u8]) -> UnitError {
    let line = last_line(out);
    let code = if code == Some(125) && line.contains(": the volume is refused: ") {
        UnitCode::ContainerOptionRefused
    } else {
        UnitCode::UnitStartFailed
    };
    UnitError::new(code, format!("podvm run: {line}"))
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

/// The exit of `podvm exec` into the container `name`. A podvm after
/// `0.2.0-beta.37` refuses an exec into a container that exited or died
/// with exit 1 (podvm T-2061); that 1 is not the exit of the step, so the
/// driver reports a container that stopped. An older podvm runs the exec.
fn exec_exit(name: &str, code: Option<i32>, out: &[u8]) -> Result<Exit, UnitError> {
    let refusal = format!("podvm exec: container {name} is not running (");
    let line = last_line(out);
    if code == Some(1) && line.starts_with(&refusal) {
        return Err(UnitError::new(UnitCode::UnitStartFailed, format!("the container stopped: {line}")));
    }
    Ok(Exit::Code(code.unwrap_or(-1)))
}

/// The first container in the output of `podvm inspect`: the JSON array
/// (or one object) after the lines that podvm writes before it.
pub(super) fn inspected(out: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(out);
    let v: Value = serde_json::from_str(&text[text.find(['[', '{'])?..]).ok()?;
    let one = if v.is_array() { v.get(0)? } else { &v };
    one.is_object().then(|| one.clone())
}

/// True when podvm says that the launcher of the container was killed:
/// the word `dead` (before `0.2.0-beta.37`) or docker's `State` object.
pub(super) fn is_dead(c: &Value) -> bool {
    let state = &c["State"];
    state == "dead" || state["Status"] == "dead" || state["Dead"] == true
}

/// The state in the output of `podvm inspect`: `State` and `ExitCode`.
/// podvm `0.2.0-beta.37` gives `State` as the object of docker
/// (`Status`, `Running`, `ExitCode`) and `ExitCode` as text; a later podvm
/// gives `ExitCode` as a number or null (podvm T-2060); an older podvm
/// gives `State` as the word. A dead container has no exit code: its end
/// is -1, so a step that waits for its exit does not wait forever.
fn state_of(out: &[u8]) -> Result<ContainerState, UnitError> {
    let c = inspected(out).ok_or_else(|| UnitError::new(UnitCode::Host, "podvm inspect gave no JSON"))?;
    let state = &c["State"];
    let running = state.as_str() == Some("running")
        || state["Status"].as_str() == Some("running")
        || state["Running"].as_bool() == Some(true);
    let code = |v: &Value| v.as_str().and_then(|s| s.parse::<i32>().ok()).or_else(|| v.as_i64().map(|n| n as i32));
    let exit = code(&state["ExitCode"]).or_else(|| code(&c["ExitCode"]));
    let exit = exit.or_else(|| is_dead(&c).then_some(-1));
    Ok(ContainerState { running, exit: if running { None } else { exit.map(Exit::Code) } })
}

#[cfg(test)]
#[path = "containers_tests.rs"]
mod tests;
