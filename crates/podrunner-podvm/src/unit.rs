//! The podvm unit: one store for the units of the driver (`PODVM_STORE`,
//! T-545) that holds each pinned image once, loaded from a tar that
//! `prepare` saved; for each unit a container with a tree of its own
//! (`podvm run -d`), its credential files in a volume of the unit, and
//! podvm's report of the run. Each process carries the tag of the unit; a
//! scan of `/proc` ends the whole tree, because a grandchild outlives
//! `podvm stop` and `rm -f`. The supervisor of podvm is a subreaper: an
//! orphan that drops the tag (`setsid env -i`) has it as its parent, so
//! the scan also takes the tree under each tagged process (T-063).

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use podrunner_schema::{Capability, UnitId};
use podrunner_unit::procs::{self, TAG_VAR};
use podrunner_unit::shim;
use podrunner_unit::{
    ContainerId, ContainerSpec, ContainerState, Count, ExecSpec, Exit, Observation, Prepared, Residue, Signal,
    StartSpec, UnitCode, UnitDriver, UnitError, parse_pinned,
};

use crate::Podvm;

mod cache;
#[cfg(test)]
mod cache_tests;
pub use cache::{tar_fault, tar_name};
#[cfg(test)]
mod destroy_tests;
mod tree_kill;
/// Each test that writes a fake podvm and runs it holds this: a fork in
/// another test can hold the new file open, and its exec fails with "Text
/// file busy".
#[cfg(test)]
pub(crate) static FAKE_PODVM: Mutex<()> = Mutex::new(());
use tree_kill::procs_comm_is_podvm;
mod confine;
mod containers;
pub(crate) mod envfile;
mod hold;
mod labels;
mod limits;
pub use limits::{MEMORY_PER_UNIT, PIDS_PER_UNIT};
mod oom;
mod root;
mod store;
#[cfg(test)]
mod store_tests;

/// The working directory of the payload in the rootfs.
const WORKDIR: &str = "/podrunner";

/// Where a unit sees its credential files (`WORKDIR/RUNNER_ROOT`).
pub const CREDENTIAL_DIR: &str = "/podrunner/.runner";

/// The runner root, under the working directory.
const RUNNER_ROOT: &str = ".runner";
/// The time that destroy waits for the processes of a unit to end.
const KILL_LIMIT: Duration = Duration::from_secs(10);

struct Running {
    dir: PathBuf,
    /// The store of the unit: the store of the units, or its own (T-545).
    store: PathBuf,
    rootfs: PathBuf,
    /// Where the credential files of the unit are on the host.
    runner_root: PathBuf,
    tag: String,
    /// `podvm wait` of the unit: its end is the end of the payload.
    waiter: Child,
    exit: Option<Exit>,
    /// The egress socket of the unit, when `podnet pod` holds it (T-065).
    egress: Option<PathBuf>,
    /// The memory of the unit holds in its cgroup leaf (T-515).
    memory_leaf: bool,
}

/// The podvm unit driver.
pub struct PodvmUnit {
    podvm: Podvm,
    state_dir: PathBuf,
    /// The store of the operator that holds the pinned images.
    images: PathBuf,
    capability: Capability,
    platform: String,
    units: Mutex<HashMap<UnitId, Running>>,
    /// The runner directory in the image: a relative mount source of a
    /// container is under it.
    runner_dir: String,
    /// The number of the next container.
    containers: std::sync::atomic::AtomicU64,
    /// The `podnet` binary of the hold (T-065), else no hold.
    /// With the backend of `podnet pod` (`netns` or `seccomp`).
    podnet: Option<(PathBuf, &'static str)>,
    /// The confine layer of podvm on each `run -d` (T-079).
    confine: bool,
    /// Why the last self-test of the layer did not prove it (T-566).
    confine_why: Option<String>,
    /// A kind with the confine layer takes no syscall tier (T-261): podvm
    /// `0.2.0-beta.34` cannot start a payload of `run -d` under both
    /// (COORD-194).
    no_tier: bool,
    /// The rung binds `run -d -v`: the socket volume (T-564), read once
    /// from the probe, before the hold and the confine add to its proof.
    volumes: bool,
    /// The rlimits of each unit (T-162).
    limits: podrunner_unit::limits::Limits,
    /// The cache of image tars, else `STATE/images` (T-242).
    tars: Option<PathBuf>,
    /// The reference that `prepare` took for each digest (T-630).
    saved: Mutex<HashMap<String, String>>,
}

fn host(what: &str, e: std::io::Error) -> UnitError {
    UnitError::new(UnitCode::Host, format!("{what}: {}", e.kind()))
}

impl PodvmUnit {
    /// A driver with its units under `state_dir` and the pinned images in
    /// the podvm store `images`. It runs `podvm probe` once.
    pub fn new(podvm: Podvm, state_dir: &Path, images: &Path) -> Result<PodvmUnit, UnitError> {
        let mut capability = podvm.probe()?;
        // The kind applies the memory and the processes that an offer or
        // its configuration names (T-198), and says where `memory` and
        // `pids` hold.
        limits::record_modes(&podvm, &mut capability);
        let platform = if capability.arch == "aarch64" { "linux/arm64" } else { "linux/amd64" }.to_string();
        let volumes = crate::probe::has_volumes(&capability);
        Ok(PodvmUnit {
            volumes,
            podvm,
            state_dir: state_dir.to_path_buf(),
            images: images.to_path_buf(),
            capability,
            platform,
            units: Mutex::new(HashMap::new()),
            runner_dir: "/home/runner".into(),
            containers: std::sync::atomic::AtomicU64::new(0),
            podnet: None,
            confine: false,
            confine_why: None,
            no_tier: false,
            limits: podrunner_unit::limits::Limits::default(),
            tars: None,
            saved: Mutex::new(HashMap::new()),
        })
    }

    /// The runner directory of the image (`[unit.podvm] runner_dir`).
    pub fn with_runner_dir(mut self, dir: &str) -> PodvmUnit {
        self.runner_dir = dir.into();
        self
    }

    /// `podvm wait` of `unit`, tagged as the unit.
    fn waiter(&self, store: &Path, unit: &UnitId, tag: &str) -> Result<Child, UnitError> {
        self.podvm
            .command()
            .args(["wait", unit.as_str()])
            .env("PODVM_STORE", store)
            .env(TAG_VAR, tag)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| host("podvm wait", e))
    }

    /// `podvm ARGS` with the store `store`; the output, or a named failure.
    fn podvm(&self, store: &Path, args: &[&str], tag: Option<&str>) -> Result<String, UnitError> {
        self.podvm_held(store, args, tag, None)
    }

    /// `podvm ARGS`, under `podnet pod` of the egress socket `egress` when
    /// the driver has a hold.
    fn podvm_held(
        &self,
        store: &Path,
        args: &[&str],
        tag: Option<&str>,
        egress: Option<&Path>,
    ) -> Result<String, UnitError> {
        let mut cmd = self.command(egress);
        cmd.args(args).env("PODVM_STORE", store).stdin(Stdio::null());
        if let Some(t) = tag {
            cmd.env(TAG_VAR, t);
        }
        let out = cmd.output().map_err(|e| host("podvm", e))?;
        if !out.status.success() {
            let verb = args.first().copied().unwrap_or("");
            let line = containers::last_line(&out.stderr);
            // A signal has no exit code: name it (SIGXFSZ, T-632).
            let why = match podrunner_unit::signal::ended_by(&out.status) {
                Some(s) if line.is_empty() => format!("podvm {verb}: {s}"),
                Some(s) => format!("podvm {verb}: {s}: {line}"),
                None => format!("podvm {verb}: exit {}: {line}", out.status.code().unwrap_or(-1)),
            };
            return Err(UnitError::new(UnitCode::UnitStartFailed, why));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn unit_dir(&self, unit: &UnitId) -> PathBuf {
        self.state_dir.join("units").join(unit.as_str())
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), UnitError> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| host("a credential file", e))?;
    f.write_all(bytes).map_err(|e| host("a credential file", e))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| host("a credential file", e))
}

impl UnitDriver for PodvmUnit {
    fn kind(&self) -> &str {
        "podvm"
    }

    fn uses_payload_runner(&self) -> bool {
        true
    }

    fn probe(&self) -> Result<Capability, UnitError> {
        Ok(self.capability.clone())
    }

    /// The socket directory of a unit is a volume where `run -d -v` binds:
    /// the supervise, userland and namespace rungs (T-050, T-564). On the
    /// namespace rung a host path is not in the root of the unit.
    fn socket_mount(&self) -> Option<&str> {
        self.volumes.then_some(containers::SOCKET_MOUNT)
    }

    fn host_mounts(&self) -> bool {
        self.volumes
    }

    fn credential_dir(&self, _unit: &UnitId) -> Option<String> {
        Some(CREDENTIAL_DIR.into())
    }

    fn prepare(&self, reference: &str, platform: &str, _emulation: bool) -> Result<Prepared, UnitError> {
        let (_, digest) = parse_pinned(reference)?;
        if platform != self.platform {
            return Err(UnitError::new(
                UnitCode::ArchMismatch,
                format!("{platform} on {}: podvm emulates nothing", self.platform),
            ));
        }
        let tar = self.tar_of(digest);
        // A start saves the image again from this reference when its tar
        // in the cache is not whole (T-630).
        self.saved.lock().expect("the saved images").insert(digest.to_string(), reference.to_string());
        if !tar.exists() {
            self.save_tar(reference, platform, &tar)?;
        }
        Ok(Prepared { digest: digest.to_string(), platform: platform.into() })
    }

    fn start(&self, spec: &StartSpec) -> Result<(), UnitError> {
        let mut units = self.units.lock().expect("the units");
        let dir = self.unit_dir(&spec.unit);
        if units.contains_key(&spec.unit) || dir.exists() {
            return Err(UnitError::new(UnitCode::UnitInUse, spec.unit.to_string()));
        }
        podrunner_unit::mounts::spec(spec)?;
        fs::create_dir_all(&dir).map_err(|e| host("the unit directory", e))?;
        let store = self.store_of(&dir);
        let reference = self.cached(&store, &spec.image.digest)?;
        let image_root = self.extracted(&store, &reference)?;
        let creds = self.credential_root(&dir, &image_root);
        self.place_credentials(&creds, spec)?;
        let tag = procs::tag_of(&self.state_dir, spec.unit.as_str());
        let mut args: Vec<String> =
            ["run", "-d", "--name", spec.unit.as_str(), "-w", WORKDIR].map(String::from).to_vec();
        // The machine name that the runner gives GitHub is the unit, not the
        // host (T-182): `--hostname` of podvm 0.2.0-beta.22 or later.
        let name = format!("podrunner-{}", spec.unit);
        args.extend(["--hostname".into(), name.clone()]);
        args.extend(labels::run_args(&self.podvm, spec.unit.as_str()));
        if !self.no_tier {
            args.extend(labels::tier_args(&self.podvm).iter().map(|a| a.to_string()));
        }
        args.extend(["--report".into(), dir.join(labels::REPORT).display().to_string()]);
        let guest = spec.sockets.as_ref().and(self.socket_mount());
        args.extend(self.confine_args(&image_root, guest, &spec.mounts));
        let mut env = spec.env.clone();
        env.insert("PODRUNNER_UNIT".into(), spec.unit.to_string());
        env.insert(TAG_VAR.into(), tag.clone());
        limits::runner_env(&mut env);
        // The lesser of the kind and the offer (T-198); the CPU only in a cgroup.
        let held = limits::held(&self.podvm, &self.limits, &spec.limits);
        let limit_args = limits::ulimit_args(&held, limits::holds(&self.podvm), &mut env);
        let memory_leaf = limit_args.iter().any(|a| a == "--memory");
        args.extend(limit_args);
        // The values in a file, not in the argv (T-405); it goes after the
        // call.
        let (env_args, _env_file) = envfile::env_args(&self.podvm, &dir, &env)?;
        args.extend(env_args);
        if let (Some(dir), Some(guest)) = (&spec.sockets, self.socket_mount()) {
            args.push("-v".into());
            args.push(format!("{}:{guest}", dir.display()));
        }
        // The credential files of a unit of the store of the units (T-545).
        if self.volumes {
            args.extend(["-v".into(), format!("{}:{CREDENTIAL_DIR}", creds.display())]);
        }
        // The host directories of the spec (T-138); `PATH:ro` is read-only.
        for (host, path) in &spec.mounts {
            args.push("-v".into());
            args.push(format!("{}:{path}", host.display()));
        }
        args.push(reference.clone());
        // The payload runs as it is: `podvm wait` gives each exit code.
        args.extend(spec.argv.iter().cloned());
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        let egress = self.egress_of(spec)?;
        match self.podvm_held(&store, &argv, Some(&tag), egress.as_deref()) {
            // A rung (or an older podvm) that refuses the name: start again
            // with no name, and say so.
            Err(e) if e.detail.contains("hostname") => {
                // No UTS namespace on the namespace rung of beta.25 (T-317).
                let why = e.detail.lines().next().unwrap_or_default();
                eprintln!("podrunner: {}: the unit has the host name of the host: {why}", spec.unit);
                let argv: Vec<&str> = labels::no_hostname(&argv, &name);
                self.podvm_held(&store, &argv, Some(&tag), egress.as_deref())?;
            }
            r => {
                r?;
            }
        }
        // The tree of the running unit (T-067), and where its credential
        // files are.
        let names: Vec<&str> = spec.credential.files.iter().map(|f| f.name.as_str()).collect();
        let inspected = self.inspected_root(&store, spec.unit.as_str());
        let (rootfs, runner_root) = self.root_of_unit(&store, &image_root, inspected, &creds, &names)?;
        let waiter = self.waiter(&store, &spec.unit, &tag)?;
        let r = Running { dir, store, rootfs, runner_root, tag, waiter, exit: None, egress, memory_leaf };
        units.insert(spec.unit.clone(), r);
        Ok(())
    }

    fn signal(&self, unit: &UnitId, signal: Signal) -> Result<(), UnitError> {
        let units = self.units.lock().expect("the units");
        let r = units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
        let sig = match signal {
            Signal::Term => libc::SIGTERM,
            Signal::Kill => libc::SIGKILL,
        };
        // Not the waiter: it reads the exit code of the payload.
        let waiter = r.waiter.id();
        for pid in procs::tagged(&r.tag).into_iter().filter(|p| *p != waiter) {
            procs::kill(pid, sig);
        }
        Ok(())
    }

    fn wait(&self, unit: &UnitId, timeout: Duration) -> Result<Option<Exit>, UnitError> {
        let end = Instant::now() + timeout;
        loop {
            {
                let mut units = self.units.lock().expect("the units");
                let r = units.get_mut(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
                if r.exit.is_none() && r.waiter.try_wait().map_err(|e| host("podvm wait", e))?.is_some() {
                    let mut out = String::new();
                    if let Some(mut o) = r.waiter.stdout.take() {
                        let _ = o.read_to_string(&mut out);
                    }
                    // `podvm wait` prints the exit code of the payload.
                    let code = out.lines().rev().find_map(|l| l.trim().parse::<i32>().ok());
                    // No code, and the payload lives: wait again.
                    let waiter = r.waiter.id();
                    let alive = procs::tagged(&r.tag).into_iter().any(|p| p != waiter);
                    match code {
                        None if alive => r.waiter = self.waiter(&r.store, unit, &r.tag)?,
                        _ => {
                            let code = code.unwrap_or(-1);
                            r.exit = Some(if code > 128 { Exit::Signal(code - 128) } else { Exit::Code(code) });
                        }
                    }
                }
                if r.exit.is_some() || Instant::now() >= end {
                    return Ok(r.exit);
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn destroy(&self, unit: &UnitId) -> Result<Residue, UnitError> {
        let removed = self.units.lock().expect("the units").remove(unit);
        let dir = self.unit_dir(unit);
        let tag = procs::tag_of(&self.state_dir, unit.as_str());
        // Each tagged process but the waiter, with its name, read once: a
        // process that ended after the list has no name, and is gone, not an
        // orphan (T-159: the name `""` of a run of the gate).
        let waiter = removed.as_ref().map(|r| r.waiter.id());
        let live = labels::live_names(&shim::tree_of_tagged(&tag), waiter);
        let orphans = labels::orphaned(&live);
        // From beta.4 the payload ends with a killed launcher, so no tagged
        // process is left; podvm names the state `dead` then. Read it before
        // the kill, which also ends the launcher.
        let dead = removed.as_ref().is_some_and(|r| self.launcher_died(&r.store, unit.as_str()));
        let ended = labels::has_exit(removed.as_ref().and_then(|r| r.exit));
        let lost = labels::count_lost(&live, dead, ended);
        if lost {
            // T-159: the cause of an unknown count, for the log of the job.
            eprintln!(
                "podrunner: destroy {unit}: no count of the processes, not a leak: orphans {orphans}, launcher dead {dead}, exit {ended}, tagged {live:?}"
            );
        }
        let left = shim::kill_tree_of_tagged(&tag, KILL_LIMIT);
        let tree = removed.as_ref().map(|r| r.rootfs.clone());
        if let Some(mut r) = removed {
            let _ = r.waiter.kill();
            let _ = r.waiter.wait();
        }
        // Each container of the unit by its label, then the unit: also of a
        // start that failed after `run -d`, before its record, and of a
        // replay (#257).
        // Each store that can hold it: also with no record (a replay).
        for store in [self.store(), dir.join(store::STORE)].iter().filter(|s| s.is_dir()) {
            self.remove_labeled(store, unit.as_str());
            let _ = self.podvm(store, &["rm", "-f", unit.as_str()], None);
        }
        // `rm -f` removes the tree of the unit; a tree that stays is
        // removed here, and counted when it still stays.
        let trees = self.store().join("trees");
        let tree = tree
            .filter(|t| t.exists() && (t.starts_with(&trees) || t.starts_with(&dir)))
            .and_then(|t| t.parent().map(Path::to_path_buf));
        for d in tree.iter().chain(Some(&dir)).filter(|d| d.exists()) {
            // A file that the payload made read-only is still removed.
            let _ = Command::new("chmod").args(["-R", "u+rwX"]).arg(d).status();
            fs::remove_dir_all(d).map_err(|e| host("remove the unit", e))?;
        }
        let files = u64::from(dir.exists()) + u64::from(tree.is_some_and(|t| t.exists()));
        let processes = if lost { Count::Unknown } else { Count::Known(left) };
        Ok(Residue { processes, files: Count::Known(files) })
    }

    fn oom_kills(&self, unit: &UnitId) -> Option<u64> {
        self.oom_count(unit)
    }

    fn observe(&self, unit: &UnitId) -> Result<Observation, UnitError> {
        let units = self.units.lock().expect("the units");
        let r = units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
        let waiter = r.waiter.id();
        let work = r.rootfs.join(WORKDIR.trim_start_matches('/'));
        // The payload first: the processes of podvm carry the tag too.
        let mut pids: Vec<u32> = shim::tree_of_tagged(&r.tag).into_iter().filter(|p| *p != waiter).collect();
        pids.sort_by_key(|p| procs_comm_is_podvm(*p));
        Ok(Observation { unit_root: Some(work), runner_root: Some(r.runner_root.clone()), pids })
    }

    fn container_start(&self, unit: &UnitId, spec: &ContainerSpec) -> Result<ContainerId, UnitError> {
        self.start_container(unit, spec)
    }
    fn container_exec(&self, unit: &UnitId, c: &ContainerId, spec: &ExecSpec) -> Result<Exit, UnitError> {
        self.exec_container(unit, c, spec)
    }
    fn container_logs(&self, unit: &UnitId, c: &ContainerId) -> Result<Vec<u8>, UnitError> {
        self.logs_container(unit, c)
    }
    fn unit_log(&self, unit: &UnitId) -> Result<podrunner_unit::UnitLog, UnitError> {
        self.log_unit(unit)
    }
    fn enter(&self, unit: &UnitId, argv: &[String], tty: bool) -> Result<std::process::Command, UnitError> {
        self.enter_unit(unit, argv, tty)
    }
    fn container_inspect(&self, unit: &UnitId, c: &ContainerId) -> Result<ContainerState, UnitError> {
        self.inspect_container(unit, c)
    }
    fn container_stop(&self, unit: &UnitId, c: &ContainerId) -> Result<(), UnitError> {
        self.stop_container(unit, c)
    }
}

#[cfg(test)]
mod credential_dir_tests {
    #[test]
    fn the_credential_dir_is_the_runner_root() {
        assert_eq!(super::CREDENTIAL_DIR, format!("{}/{}", super::WORKDIR, super::RUNNER_ROOT));
    }
}
