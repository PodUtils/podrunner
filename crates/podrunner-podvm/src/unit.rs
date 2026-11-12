//! The podvm unit: a store of its own for each unit (`PODVM_STORE`), the
//! image loaded into it from a tar that `prepare` saved, the credential
//! files written into the rootfs that `podvm extract` names, and the
//! payload started detached. Each process carries the tag of the unit; a
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

mod confine;
mod containers;
mod hold;
mod labels;
mod root;

/// The working directory of the payload in the rootfs.
const WORKDIR: &str = "/podrunner";

/// The runner root, under the working directory.
const RUNNER_ROOT: &str = ".runner";
/// The time that destroy waits for the processes of a unit to end.
const KILL_LIMIT: Duration = Duration::from_secs(10);

struct Running {
    dir: PathBuf,
    rootfs: PathBuf,
    tag: String,
    /// `podvm wait` of the unit: its end is the end of the payload.
    waiter: Child,
    exit: Option<Exit>,
    /// The egress socket of the unit, when `podnet pod` holds it (T-065).
    egress: Option<PathBuf>,
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
}

fn host(what: &str, e: std::io::Error) -> UnitError {
    UnitError::new(UnitCode::Host, format!("{what}: {}", e.kind()))
}

impl PodvmUnit {
    /// A driver with its units under `state_dir` and the pinned images in
    /// the podvm store `images`. It runs `podvm probe` once.
    pub fn new(podvm: Podvm, state_dir: &Path, images: &Path) -> Result<PodvmUnit, UnitError> {
        let capability = podvm.probe()?;
        let platform = if capability.arch == "aarch64" { "linux/arm64" } else { "linux/amd64" }.to_string();
        Ok(PodvmUnit {
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
        })
    }

    /// The runner directory of the image (`[unit.podvm] runner_dir`).
    pub fn with_runner_dir(mut self, dir: &str) -> PodvmUnit {
        self.runner_dir = dir.into();
        self
    }

    /// `podvm wait` of `unit`, tagged as the unit.
    fn waiter(&self, store: &Path, unit: &UnitId, tag: &str) -> Result<Child, UnitError> {
        Command::new(&self.podvm.binary)
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
            return Err(UnitError::new(
                UnitCode::UnitStartFailed,
                format!(
                    "podvm {verb}: exit {}: {}",
                    out.status.code().unwrap_or(-1),
                    containers::last_line(&out.stderr)
                ),
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn tar_of(&self, digest: &str) -> PathBuf {
        self.state_dir.join("images").join(format!("{}.tar", digest.trim_start_matches("sha256:")))
    }

    fn unit_dir(&self, unit: &UnitId) -> PathBuf {
        self.state_dir.join("units").join(unit.as_str())
    }
}

fn write_private(path: &Path, text: &str) -> Result<(), UnitError> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| host("a credential file", e))?;
    f.write_all(text.as_bytes()).map_err(|e| host("a credential file", e))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| host("a credential file", e))
}

impl UnitDriver for PodvmUnit {
    fn kind(&self) -> &str {
        "podvm"
    }

    fn probe(&self) -> Result<Capability, UnitError> {
        Ok(self.capability.clone())
    }

    /// The socket directory of a unit is a volume where volumes work: on
    /// the supervise rung (T-050), the same rung that has containers.
    fn socket_mount(&self) -> Option<&str> {
        (self.capability.container == podrunner_schema::ContainerRoute::Hooks).then_some(containers::SOCKET_MOUNT)
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
        if !tar.exists() {
            fs::create_dir_all(tar.parent().expect("a directory")).map_err(|e| host("the image cache", e))?;
            let part = tar.with_extension("part");
            self.podvm(&self.images, &["save", "-o", &part.to_string_lossy(), reference], None)?;
            fs::rename(&part, &tar).map_err(|e| host("the image cache", e))?;
        }
        Ok(Prepared { digest: digest.to_string(), platform: platform.into() })
    }

    fn start(&self, spec: &StartSpec) -> Result<(), UnitError> {
        let mut units = self.units.lock().expect("the units");
        let dir = self.unit_dir(&spec.unit);
        if units.contains_key(&spec.unit) || dir.exists() {
            return Err(UnitError::new(UnitCode::UnitInUse, spec.unit.to_string()));
        }
        let store = dir.join("store");
        fs::create_dir_all(&store).map_err(|e| host("the unit store", e))?;
        let tar = self.tar_of(&spec.image.digest);
        self.podvm(&store, &["load", "-i", &tar.to_string_lossy()], None)?;
        let reference = self
            .image_reference(&store)
            .ok_or_else(|| UnitError::new(UnitCode::UnitStartFailed, "podvm load gave no image"))?;
        let out = self.podvm(&store, &["extract", &reference], None)?;
        let rootfs = PathBuf::from(out.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim());
        if !rootfs.is_dir() {
            return Err(UnitError::new(UnitCode::UnitStartFailed, "podvm extract names no rootfs"));
        }
        let runner_root = rootfs.join(WORKDIR.trim_start_matches('/')).join(RUNNER_ROOT);
        fs::create_dir_all(&runner_root).map_err(|e| host("the runner root", e))?;
        fs::set_permissions(&runner_root, fs::Permissions::from_mode(0o700)).map_err(|e| host("the runner root", e))?;
        for f in &spec.credential.files {
            if !f.name_is_plain() {
                return Err(UnitError::new(UnitCode::UnitStartFailed, "a credential file name that is not plain"));
            }
            write_private(&runner_root.join(&f.name), f.content.expose())?;
        }
        let tag = procs::tag_of(&self.state_dir, spec.unit.as_str());
        let mut args: Vec<String> =
            ["run", "-d", "--name", spec.unit.as_str(), "-w", WORKDIR].map(String::from).to_vec();
        args.extend(labels::label_args(spec.unit.as_str()));
        args.extend(["--report".into(), dir.join(labels::REPORT).display().to_string()]);
        let guest = spec.sockets.as_ref().and(self.socket_mount());
        args.extend(self.confine_args(&rootfs, guest));
        let mut env = spec.env.clone();
        env.insert("PODRUNNER_UNIT".into(), spec.unit.to_string());
        env.insert(TAG_VAR.into(), tag.clone());
        // The .NET runner dies at start under a small RLIMIT_FSIZE with W^X on.
        env.insert("DOTNET_EnableWriteXorExecute".into(), "0".into());
        for (k, v) in &env {
            args.push("-e".into());
            args.push(format!("{k}={v}"));
        }
        if let (Some(dir), Some(guest)) = (&spec.sockets, self.socket_mount()) {
            args.push("-v".into());
            args.push(format!("{}:{guest}", dir.display()));
        }
        args.push(reference.clone());
        // `podvm wait` gives each exit code (PodUtils/podvm@T-1720), so the
        // payload runs as it is.
        args.extend(spec.argv.iter().cloned());
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        let egress = self.egress_of(spec)?;
        self.podvm_held(&store, &argv, Some(&tag), egress.as_deref())?;
        // The tree of the running unit (T-067): the copy of a later podvm.
        let names: Vec<&str> = spec.credential.files.iter().map(|f| f.name.as_str()).collect();
        let rootfs = root::root_of_unit(&rootfs, self.inspected_root(&store, spec.unit.as_str()), &names)?;
        let waiter = self.waiter(&store, &spec.unit, &tag)?;
        units.insert(spec.unit.clone(), Running { dir, rootfs, tag, waiter, exit: None, egress });
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
                        None if alive => r.waiter = self.waiter(&r.dir.join("store"), unit, &r.tag)?,
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
        // With no live supervisor of podvm, but processes of the unit, the
        // orphans went to init: no count is true then (design test 15).
        let tree = shim::tree_of_tagged(&tag);
        let waiter = removed.as_ref().map(|r| r.waiter.id());
        // The `podnet pod` of the hold carries the tag and can outlive the
        // payload: it is no process of the payload.
        let lost = !tree.iter().any(|p| Some(*p) != waiter && procs_comm_is_podvm(*p))
            && tree.iter().any(|p| Some(*p) != waiter && !procs_comm_is(*p, "podnet"));
        // From beta.4 the payload ends with a killed launcher, so no tagged
        // process is left; podvm names the state `dead` then. Read it before
        // the kill, which also ends the launcher.
        let lost = lost || removed.as_ref().is_some_and(|r| self.launcher_died(&r.dir.join("store"), unit.as_str()));
        let left = shim::kill_tree_of_tagged(&tag, KILL_LIMIT);
        if let Some(mut r) = removed {
            let _ = r.waiter.kill();
            let _ = r.waiter.wait();
            let store = r.dir.join("store");
            // Each container of the unit by its label, then the unit.
            self.remove_labeled(&store, unit.as_str());
            let _ = self.podvm(&store, &["rm", "-f", unit.as_str()], None);
        }
        if dir.exists() {
            // A file that the payload made read-only is still removed.
            let _ = Command::new("chmod").args(["-R", "u+rwX"]).arg(&dir).status();
            fs::remove_dir_all(&dir).map_err(|e| host("remove the unit", e))?;
        }
        let processes = if lost { Count::Unknown } else { Count::Known(left) };
        Ok(Residue { processes, files: Count::Known(u64::from(dir.exists())) })
    }

    fn observe(&self, unit: &UnitId) -> Result<Observation, UnitError> {
        let units = self.units.lock().expect("the units");
        let r = units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
        let waiter = r.waiter.id();
        let work = r.rootfs.join(WORKDIR.trim_start_matches('/'));
        // The payload first: the processes of podvm carry the tag too.
        let mut pids: Vec<u32> = shim::tree_of_tagged(&r.tag).into_iter().filter(|p| *p != waiter).collect();
        pids.sort_by_key(|p| procs_comm_is_podvm(*p));
        Ok(Observation { unit_root: Some(work.clone()), runner_root: Some(work.join(RUNNER_ROOT)), pids })
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
    fn container_inspect(&self, unit: &UnitId, c: &ContainerId) -> Result<ContainerState, UnitError> {
        self.inspect_container(unit, c)
    }
    fn container_stop(&self, unit: &UnitId, c: &ContainerId) -> Result<(), UnitError> {
        self.stop_container(unit, c)
    }
}

/// True when the process is podvm itself (its name).
fn procs_comm_is_podvm(pid: u32) -> bool {
    procs_comm_is(pid, "podvm")
}

fn procs_comm_is(pid: u32, name: &str) -> bool {
    fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|c| c.trim().starts_with(name))
}

impl PodvmUnit {
    /// Prove the tree kill on this host with the pinned image `reference`
    /// (unit test 3): a payload with a `setsid` grandchild, then destroy.
    /// When no process is left, the record says `tree_kill: verified`, and
    /// a `wall` limit can be enforced. The image needs `sh`, `setsid` and
    /// `sleep`.
    pub fn verify_tree_kill(&mut self, reference: &str, unit: UnitId) -> Result<bool, UnitError> {
        let image = self.prepare(reference, &self.platform.clone(), false)?;
        let argv = ["sh", "-c", "setsid sh -c 'sleep 300 & wait' & wait"].map(String::from).to_vec();
        let spec = StartSpec {
            unit: unit.clone(),
            image,
            argv,
            env: Default::default(),
            credential: Default::default(),
            sockets: None,
        };
        self.start(&spec)?;
        let tag = procs::tag_of(&self.state_dir, unit.as_str());
        let end = Instant::now() + Duration::from_secs(10);
        let mut seen = Vec::new();
        while Instant::now() < end {
            seen = procs::tagged(&tag);
            if seen.iter().filter(|p| !procs_comm_is_podvm(**p)).count() >= 3 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let residue = self.destroy(&unit)?;
        let verified =
            seen.len() >= 3 && residue.is_clean() && seen.iter().all(|p| !podrunner_unit::conformance::alive(*p));
        if verified {
            self.capability.tree_kill = podrunner_schema::TreeKill::Verified;
            self.capability.limits.insert("wall".into(), podrunner_schema::LimitMode::Enforced);
        }
        Ok(verified)
    }

    /// The reference of the one image of a unit store: `podvm images`
    /// names the repository and the tag that `load` gave it.
    fn image_reference(&self, store: &Path) -> Option<String> {
        let out = self.podvm(store, &["images"], None).ok()?;
        let line = out.lines().nth(1)?;
        let mut cols = line.split_whitespace();
        Some(format!("{}:{}", cols.next()?, cols.next()?))
    }
}
