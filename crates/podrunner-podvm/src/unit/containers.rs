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
//! redirect the bind (T-403). Each container
//! carries the tag of its unit, so the tree kill of `destroy` reaches it.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
        for m in &spec.mounts {
            // A relative source is under the runner directory of the unit.
            let source = if m.source.is_absolute() { m.source.clone() } else { runner.join(&m.source) };
            if !m.source.is_absolute() {
                no_link(&runner, &m.source)?;
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

    /// podvm `0.2.0-beta.37` gives `State` as the object of docker, and
    /// `ExitCode` as a number or `-` (the output of that release).
    #[test]
    fn the_state_comes_from_the_docker_object_of_inspect() {
        let up = br#"[{"ExitCode":"-","Name":"c1","State":{"Dead":false,"ExitCode":null,"Pid":20170,"Running":true,"Status":"running"},"Status":"Up"}]"#;
        assert_eq!(state_of(up).unwrap(), ContainerState { running: true, exit: None });
        let done = br#"[{"ExitCode":3,"Name":"c2","State":{"Dead":false,"ExitCode":3,"Pid":0,"Running":false,"Status":"exited"},"Status":"Exited (3)"}]"#;
        assert_eq!(state_of(done).unwrap(), ContainerState { running: false, exit: Some(Exit::Code(3)) });
    }

    /// T-581: a podvm after `0.2.0-beta.37` gives docker's types (podvm
    /// T-2060): `ExitCode` and `Pid` are numbers or null, `Labels` is a
    /// map, `Config` is an object. A dead container has no exit code in
    /// each form: its end is -1, not a wait with no end.
    #[test]
    fn the_state_comes_from_the_typed_inspect_of_a_later_podvm() {
        let doc = |state: &str, exit: &str| {
            format!(
                r#"[{{"Id":"a","Name":"u-1-c0","State":{state},"Status":"x","Pid":77,"LauncherPid":70,"ExitCode":{exit},"RootfsPath":"/s/r","Labels":{{"podrunner.unit":"u-1"}},"Complete.Fixups":"-","Config":{{"Env":["A=1"],"Cmd":["tail","-f","/dev/null"],"Image":"img","WorkingDir":"/w","Labels":{{"podrunner.unit":"u-1"}}}},"End":{{"Reason":"-","RunMs":null}}}}]"#
            )
        };
        let up = doc(r#"{"Status":"running","Running":true,"Dead":false,"ExitCode":null,"Pid":77}"#, "null");
        assert_eq!(state_of(up.as_bytes()).unwrap(), ContainerState { running: true, exit: None });
        let done = doc(r#"{"Status":"exited","Running":false,"Dead":false,"ExitCode":3,"Pid":0}"#, "3");
        assert_eq!(state_of(done.as_bytes()).unwrap(), ContainerState { running: false, exit: Some(Exit::Code(3)) });
        let created = doc(r#"{"Status":"created","Running":false,"Dead":false,"ExitCode":null,"Pid":0}"#, "null");
        assert_eq!(state_of(created.as_bytes()).unwrap(), ContainerState { running: false, exit: None });
        let dead = doc(r#"{"Status":"dead","Running":false,"Dead":true,"ExitCode":null,"Pid":0}"#, "null");
        assert_eq!(state_of(dead.as_bytes()).unwrap(), ContainerState { running: false, exit: Some(Exit::Code(-1)) });
        // `0.2.0-beta.37` and an older podvm: a dead container, no code.
        let dead37 =
            br#"[{"ExitCode":"-","Name":"c","State":{"Dead":true,"ExitCode":null,"Running":false,"Status":"dead"}}]"#;
        assert_eq!(state_of(dead37).unwrap().exit, Some(Exit::Code(-1)));
        let dead_word = br#"[{"Name":"c","State":"dead","ExitCode":"-"}]"#;
        assert_eq!(state_of(dead_word).unwrap().exit, Some(Exit::Code(-1)));
    }

    /// T-581: the check of a killed launcher (`launcher_died`) reads each
    /// form of `State`, after a line that podvm writes before the JSON.
    #[test]
    fn a_dead_launcher_is_seen_in_each_form_of_inspect() {
        let dead = |out: &str| inspected(out.as_bytes()).is_some_and(|c| is_dead(&c));
        assert!(dead(r#"[{"Name":"u-1","State":"dead"}]"#), "the word");
        assert!(dead(r#"[{"Name":"u-1","State":{"Status":"dead","Dead":true,"ExitCode":null}}]"#), "docker's object");
        assert!(
            dead("u-1: Extracted\n[{\"State\":{\"Status\":\"dead\"},\"Config\":{\"Cmd\":[\"sh\"]}}]"),
            "a line before"
        );
        assert!(dead(r#"{"State":{"Dead":true}}"#), "one object");
        assert!(!dead(r#"[{"State":{"Status":"exited","Dead":false,"ExitCode":0}}]"#));
        assert!(!dead(r#"[{"State":"running"}]"#));
        assert!(!dead("Error: no such container u-1\n"));
    }

    /// T-581: a podvm after `0.2.0-beta.37` refuses `exec` into a container
    /// that ended with exit 1 (podvm T-2061). That is a stopped container,
    /// not a step that exits 1. The exit of a step stays its exit.
    #[test]
    fn a_refused_exec_into_an_ended_container_is_not_the_exit_of_the_step() {
        let refusal =
            b"podvm exec: container u-1-c0 is not running (exited). Start it with `podvm start u-1-c0`\n".as_slice();
        let e = exec_exit("u-1-c0", Some(1), refusal).unwrap_err();
        assert_eq!(e.code, UnitCode::UnitStartFailed);
        assert!(e.detail.contains("is not running (exited)"), "{}", e.detail);
        let dead = b"podvm exec: container u-1-c0 is not running (dead). Start it with `podvm start u-1-c0`".as_slice();
        assert!(exec_exit("u-1-c0", Some(1), dead).is_err());
        // `0.2.0-beta.37` runs the exec: the step's own exit 1 stays.
        assert_eq!(exec_exit("u-1-c0", Some(1), b"make: *** [all] Error 1\n").unwrap(), Exit::Code(1));
        // The text of another container, or another code, is a step's output.
        assert_eq!(exec_exit("u-1-c1", Some(1), refusal).unwrap(), Exit::Code(1));
        assert_eq!(exec_exit("u-1-c0", Some(0), refusal).unwrap(), Exit::Code(0));
        assert_eq!(exec_exit("u-1-c0", Some(126), b"").unwrap(), Exit::Code(126));
        assert_eq!(exec_exit("u-1-c0", None, b"").unwrap(), Exit::Code(-1));
    }

    /// T-532, T-403: a container of the namespace rung binds its volumes
    /// under the runner directory with no symbolic link; the other rungs
    /// get no flag.
    #[test]
    fn a_container_of_the_namespace_rung_has_the_volume_base() {
        let runner = Path::new("/u/store/trees/t/rootfs/home/runner");
        assert_eq!(
            volume_base_args(Some("namespace"), runner),
            ["--podvm-volume-base", "/u/store/trees/t/rootfs/home/runner"]
        );
        for rung in [Some("supervise"), Some("userland"), None] {
            assert!(volume_base_args(rung, runner).is_empty(), "{rung:?}");
        }
    }

    /// T-403: a link in a mount source under the runner directory is
    /// refused before podvm runs; a link out of it is not the job's.
    #[test]
    fn a_linked_source_under_the_runner_directory_is_refused() {
        let dir = std::env::temp_dir().join(format!("podrunner-podvm-nolink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("runner/_work/_temp")).unwrap();
        std::os::unix::fs::symlink("/", dir.join("runner/_work/_temp/_github_home")).unwrap();
        let runner = dir.join("runner");
        assert!(no_link(&runner, Path::new("_work")).is_ok());
        assert!(no_link(&runner, Path::new("_work/_temp/absent/x")).is_ok());
        for rel in ["_work/_temp/_github_home", "_work/_temp/_github_home/x"] {
            let e = no_link(&runner, Path::new(rel)).unwrap_err();
            assert_eq!(e.code, UnitCode::ContainerOptionRefused, "{rel}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-403: the refusal of podvm `0.2.0-beta.37` for a link under the
    /// volume base (measured on the namespace rung) is a refused option;
    /// another exit 125 stays a start that failed.
    #[test]
    fn a_volume_that_podvm_refuses_is_a_refused_option() {
        let out = b"podvm run: -v /u/rootfs/home/runner/_work/_temp/_github_home:/github/home: the volume is refused: \
                    its host path holds a symbolic link under its base /u/rootfs/home/runner (--podvm-volume-base); \
                    it resolves to /\n";
        let e = run_error(Some(125), out);
        assert_eq!(e.code, UnitCode::ContainerOptionRefused);
        assert!(e.detail.contains("the volume is refused"), "{}", e.detail);
        assert_eq!(run_error(Some(1), out).code, UnitCode::UnitStartFailed);
        assert_eq!(
            run_error(Some(125), b"podvm run: chroot(2) is denied on this machine\n").code,
            UnitCode::UnitStartFailed
        );
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
