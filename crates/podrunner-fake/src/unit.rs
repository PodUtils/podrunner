//! The `fake` unit: it declares any capability record and runs the payload
//! as real processes on the host, in a directory of its own, so that the
//! tree kill and the residue counts are real. It is no isolation: the
//! configuration refuses it in production unless `allow_fake = true`.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use podrunner_schema::{Capability, TreeKill, UnitId};
use podrunner_unit::{
    ContainerId, ContainerSpec, ContainerState, Count, ExecSpec, Exit, Observation, Prepared, Residue, Signal,
    StartSpec, UnitCode, UnitDriver, UnitError, parse_pinned,
};

use podrunner_unit::procs::{self, TAG_VAR};
use podrunner_unit::shim::{self, Mode};

use crate::mounts::Mounts;

mod containers;

/// The planted defects of the fake unit: each one fails the unit
/// conformance test of its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitPlant {
    /// 1: each probe gives another record.
    ProbeDrifts,
    /// 2: a tag is accepted.
    AcceptTag,
    /// 3: destroy kills the top process only.
    KillTopOnly,
    /// 4: a foreign platform is accepted.
    AcceptForeign,
    /// 5: each unit gets the same root.
    SharedRoot,
    /// 6: the credential files have mode 0644.
    LooseMode,
    /// 7: the credential goes into the environment.
    CredentialInEnv,
    /// 8: wait gives exit code 0.
    WrongCode,
    /// 9: signal does nothing.
    DeafSignal,
    /// 10: destroy kills nothing.
    NoKill,
    /// 11: destroy keeps the files.
    KeepFiles,
    /// 12: destroy counts nothing (`unknown`).
    UnknownResidue,
    /// 13: a container starts with no route.
    ContainerWithNoRoute,
    /// 14: exec accepts stdin.
    AcceptStdin,
    /// 15: a second destroy fails.
    DestroyOnce,
    /// 16: a unit id in use starts again.
    StartTwice,
    /// 17: probe claims a verified tree kill, and destroy kills the top
    /// process only.
    FalseTreeKill,
    /// 18: probe writes a file in the state directory.
    ProbeWritesFile,
    /// 19: each unit gets one shared `/etc/hosts`.
    SharedHosts,
    /// 20: the processes of a container do not carry the tag of the unit.
    UntaggedContainer,
    /// 21: start keeps a copy of the credential outside the unit root.
    CredentialCopy,
    /// 22: probe gives the credential of the last unit in `boundary_proof`.
    ProofCanary,
    /// 23: probe leaves a process.
    ProbeSpawns,
    /// 24: the credential goes into the environment of each container.
    CredentialInContainers,
    /// 25: no shim: destroy finds the processes by their tag only.
    TagOnly,
    /// 26: destroy counts 0 processes after its shim was killed.
    ShimDeathIgnored,
    /// 27: the record claims `confined`, and no rule applies.
    ClaimsConfined,
}

struct Running {
    root: PathBuf,
    runner_root: PathBuf,
    tag: String,
    child: Child,
    exit: Option<Exit>,
    /// Each container: its process, its directory and its mounts.
    containers: HashMap<ContainerId, (Child, PathBuf, Mounts)>,
}

/// The `fake` unit driver.
pub struct FakeUnit {
    state_dir: PathBuf,
    capability: Capability,
    platform: String,
    plant: Option<UnitPlant>,
    units: Mutex<HashMap<UnitId, Running>>,
    probes: AtomicU64,
    containers: AtomicU64,
    /// Each tag that this driver gave: the drop kills what is left of them.
    tags: Mutex<Vec<String>>,
    /// The first credential file of the last start (plants 22 and 24).
    seen: Mutex<Option<String>>,
    /// The Landlock ABI of a `confined` fake (T-042).
    confined: Option<u32>,
}

impl Drop for FakeUnit {
    /// The plants leave processes behind; a fake leaves none on the host.
    fn drop(&mut self) {
        if let Ok(units) = self.units.get_mut() {
            for r in units.values_mut() {
                shim::kill_unit(r.child.id(), Some(&r.tag), KILL_LIMIT);
                let _ = r.child.kill();
                let _ = r.child.wait();
            }
        }
        for tag in self.tags.lock().map(|t| t.clone()).unwrap_or_default() {
            procs::kill_tagged(&tag, KILL_LIMIT);
        }
    }
}

/// The time that destroy waits for the processes of a unit to end.
const KILL_LIMIT: Duration = Duration::from_secs(5);

fn host(e: std::io::Error, what: &str) -> UnitError {
    UnitError::new(UnitCode::Host, format!("{what}: {}", e.kind()))
}

impl FakeUnit {
    /// A fake unit with its units under `state_dir`, the record
    /// `capability`, on the host platform `platform` (`linux/amd64`).
    pub fn new(state_dir: &Path, mut capability: Capability, platform: &str) -> FakeUnit {
        capability.kind = "fake".into();
        FakeUnit {
            state_dir: state_dir.to_path_buf(),
            capability,
            platform: platform.into(),
            plant: None,
            units: Mutex::new(HashMap::new()),
            probes: AtomicU64::new(0),
            containers: AtomicU64::new(0),
            tags: Mutex::new(Vec::new()),
            seen: Mutex::new(None),
            confined: None,
        }
    }

    /// A `confined` fake (T-042): each process of a unit runs under the
    /// rules of `podrunner_unit::confine`, and the record says
    /// `boundary: confined` with its proof. Refused with no Landlock.
    pub fn confined(mut self) -> Result<FakeUnit, String> {
        let abi = podrunner_unit::confine::check()?;
        self.capability.boundary = podrunner_schema::Boundary::Confined;
        self.capability.boundary_proof = Some(podrunner_unit::confine::proof(abi));
        self.confined = Some(abi);
        Ok(self)
    }

    pub fn with_plant(mut self, plant: UnitPlant) -> FakeUnit {
        if plant == UnitPlant::FalseTreeKill {
            self.capability.tree_kill = TreeKill::Verified;
        }
        if plant == UnitPlant::ClaimsConfined {
            self.capability.boundary = podrunner_schema::Boundary::Confined;
        }
        self.plant = Some(plant);
        self
    }

    fn planted(&self, p: UnitPlant) -> bool {
        self.plant == Some(p)
    }

    /// The top process of a unit runs under a shim (T-063), but for the
    /// plants that kill by the tag or the top process only.
    fn shimmed(&self) -> bool {
        !matches!(self.plant, Some(UnitPlant::KillTopOnly | UnitPlant::FalseTreeKill | UnitPlant::TagOnly))
    }

    fn with_unit<T>(
        &self,
        unit: &UnitId,
        f: impl FnOnce(&mut Running) -> Result<T, UnitError>,
    ) -> Result<T, UnitError> {
        let mut units = self.units.lock().expect("the units");
        let r = units.get_mut(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
        f(r)
    }

    fn spawn(
        &self,
        argv: &[String],
        dir: &Path,
        env: &[(String, String)],
        log: Option<&Path>,
        shimmed: bool,
        root: Option<&Path>,
    ) -> Result<Child, UnitError> {
        let (prog, args) =
            argv.split_first().ok_or_else(|| UnitError::new(UnitCode::UnitStartFailed, "an empty argv"))?;
        let mut cmd = Command::new(prog);
        cmd.args(args).current_dir(dir).env_clear().stdin(Stdio::null());
        cmd.env("PATH", std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()));
        for (k, v) in env {
            cmd.env(k, v);
        }
        match log {
            Some(path) => {
                let f = fs::File::create(path).map_err(|e| host(e, "the log of a container"))?;
                cmd.stdout(f.try_clone().map_err(|e| host(e, "the log"))?).stderr(f);
            }
            None => {
                cmd.stdout(Stdio::null()).stderr(Stdio::null());
            }
        }
        // A confined fake confines each process of a unit to its root.
        let confine = match (self.confined, root) {
            (Some(abi), Some(root)) => Some(
                podrunner_unit::confine::for_unit(root, abi)
                    .map_err(|e| UnitError::new(UnitCode::UnitStartFailed, e))?,
            ),
            _ => None,
        };
        if shimmed {
            shim::under_shim_confined(&mut cmd, Mode::EndWithPayload, confine);
        }
        cmd.spawn().map_err(|e| UnitError::new(UnitCode::UnitStartFailed, format!("spawn: {}", e.kind())))
    }
}

impl FakeUnit {
    /// The plants 19, 21, 22 and 24 at start.
    fn plant_start(&self, spec: &StartSpec, root: &Path) -> Result<(), UnitError> {
        let first = spec.credential.files.first().map(|f| f.content.expose().to_string());
        if self.planted(UnitPlant::SharedHosts) {
            fs::create_dir_all(root.join("etc")).map_err(|e| host(e, "etc"))?;
            let _ = std::os::unix::fs::symlink(self.state_dir.join("hosts"), root.join("etc/hosts"));
        }
        if self.planted(UnitPlant::CredentialCopy)
            && let Some(text) = &first
        {
            fs::write(self.state_dir.join(format!("copy-{}", spec.unit)), text).map_err(|e| host(e, "a copy"))?;
        }
        if self.planted(UnitPlant::ProofCanary) || self.planted(UnitPlant::CredentialInContainers) {
            *self.seen.lock().expect("the credential seen") = first;
        }
        Ok(())
    }
}

fn write_private(path: &Path, text: &str, mode: u32) -> Result<(), UnitError> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(path)
        .map_err(|e| host(e, "a credential file"))?;
    f.write_all(text.as_bytes()).map_err(|e| host(e, "a credential file"))?;
    // The umask can take bits away from the mode of open(2), not add them.
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|e| host(e, "a credential file"))
}

impl UnitDriver for FakeUnit {
    fn kind(&self) -> &str {
        "fake"
    }

    fn probe(&self) -> Result<Capability, UnitError> {
        let mut c = self.capability.clone();
        if self.planted(UnitPlant::ProbeDrifts) {
            c.boundary_proof = Some(format!("probe-{}", self.probes.fetch_add(1, Ordering::Relaxed)));
        }
        if self.planted(UnitPlant::ProbeWritesFile) {
            let n = self.probes.fetch_add(1, Ordering::Relaxed);
            fs::write(self.state_dir.join(format!("probe-{n}")), "").map_err(|e| host(e, "probe"))?;
        }
        if self.planted(UnitPlant::ProofCanary) {
            c.boundary_proof = self.seen.lock().expect("the credential seen").clone();
        }
        if self.planted(UnitPlant::ProbeSpawns) {
            let tag = procs::tag_of(&self.state_dir, "probe");
            self.tags.lock().expect("the tags").push(tag.clone());
            self.spawn(&["sleep".into(), "30".into()], &self.state_dir, &[(TAG_VAR.into(), tag)], None, false, None)?;
        }
        Ok(c)
    }

    fn prepare(&self, reference: &str, platform: &str, emulation: bool) -> Result<Prepared, UnitError> {
        let digest = match parse_pinned(reference) {
            Ok((_, d)) => d.to_string(),
            Err(_) if self.planted(UnitPlant::AcceptTag) => "sha256:".to_string() + &"0".repeat(64),
            Err(e) => return Err(e),
        };
        if platform != self.platform && !self.planted(UnitPlant::AcceptForeign) {
            let why = if emulation { "the fake unit emulates nothing" } else { "no emulation named" };
            return Err(UnitError::new(UnitCode::ArchMismatch, format!("{platform} on {}: {why}", self.platform)));
        }
        Ok(Prepared { digest, platform: platform.into() })
    }

    fn start(&self, spec: &StartSpec) -> Result<(), UnitError> {
        let mut units = self.units.lock().expect("the units");
        if units.contains_key(&spec.unit) && !self.planted(UnitPlant::StartTwice) {
            return Err(UnitError::new(UnitCode::UnitInUse, spec.unit.to_string()));
        }
        let name = if self.planted(UnitPlant::SharedRoot) { "shared" } else { spec.unit.as_str() };
        let root = self.state_dir.join("units").join(name);
        let runner_root = root.join(".runner-root");
        let fresh = !runner_root.exists();
        fs::create_dir_all(&runner_root).map_err(|e| host(e, "the unit root"))?;
        fs::set_permissions(&runner_root, fs::Permissions::from_mode(0o700)).map_err(|e| host(e, "the runner root"))?;
        let mode = if self.planted(UnitPlant::LooseMode) { 0o644 } else { 0o600 };
        for f in spec.credential.files.iter().filter(|_| fresh) {
            if !f.name_is_plain() {
                return Err(UnitError::new(UnitCode::UnitStartFailed, "a credential file name that is not plain"));
            }
            write_private(&runner_root.join(&f.name), f.content.expose(), mode)?;
        }
        self.plant_start(spec, &root)?;
        let tag = procs::tag_of(&self.state_dir, spec.unit.as_str());
        self.tags.lock().expect("the tags").push(tag.clone());
        let mut env: Vec<(String, String)> = spec.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        env.push(("PODRUNNER_UNIT".into(), spec.unit.to_string()));
        env.push((TAG_VAR.into(), tag.clone()));
        if self.planted(UnitPlant::CredentialInEnv)
            && let Some(f) = spec.credential.files.first()
        {
            env.push(("PODRUNNER_PLANTED_CREDENTIAL".into(), f.content.expose().to_string()));
        }
        let child = self.spawn(&spec.argv, &root, &env, None, self.shimmed(), Some(&root))?;
        let run = Running { root, runner_root, tag, child, exit: None, containers: HashMap::new() };
        units.insert(spec.unit.clone(), run);
        Ok(())
    }

    fn signal(&self, unit: &UnitId, signal: Signal) -> Result<(), UnitError> {
        let deaf = self.planted(UnitPlant::DeafSignal);
        self.with_unit(unit, |r| {
            let sig = match signal {
                Signal::Term => libc::SIGTERM,
                Signal::Kill => libc::SIGKILL,
            };
            for pid in procs::tagged(&r.tag).into_iter().filter(|_| !deaf) {
                procs::kill(pid, sig);
            }
            Ok(())
        })
    }

    fn wait(&self, unit: &UnitId, timeout: Duration) -> Result<Option<Exit>, UnitError> {
        let end = Instant::now() + timeout;
        let wrong = self.planted(UnitPlant::WrongCode);
        loop {
            let got = self.with_unit(unit, |r| {
                if r.exit.is_none()
                    && let Some(status) = r.child.try_wait().map_err(|e| host(e, "wait"))?
                {
                    r.exit = Some(if wrong { Exit::Code(0) } else { procs::exit_of(status) });
                }
                Ok(r.exit)
            })?;
            if got.is_some() || Instant::now() >= end {
                return Ok(got);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn destroy(&self, unit: &UnitId) -> Result<Residue, UnitError> {
        let Some(mut r) = self.units.lock().expect("the units").remove(unit) else {
            if self.planted(UnitPlant::DestroyOnce) {
                return Err(UnitError::new(UnitCode::UnknownUnit, unit.to_string()));
            }
            // A unit of an earlier process, or one that is gone: find what
            // is left by its tag and its directory.
            let left = procs::kill_tagged(&procs::tag_of(&self.state_dir, unit.as_str()), KILL_LIMIT);
            let root = self.state_dir.join("units").join(unit.as_str());
            let _ = fs::remove_dir_all(&root);
            let files = Count::Known(u64::from(root.exists()));
            return Ok(Residue { processes: Count::Known(left), files });
        };
        let top_only = self.planted(UnitPlant::KillTopOnly) || self.planted(UnitPlant::FalseTreeKill);
        // A shim that a signal ended before destroy lost its orphans to
        // init: no count is true then (design test 15).
        let shim_lost = self.shimmed()
            && !self.planted(UnitPlant::ShimDeathIgnored)
            && r.child
                .try_wait()
                .ok()
                .flatten()
                .is_some_and(|s| std::os::unix::process::ExitStatusExt::signal(&s).is_some());
        for (_, (mut c, _, _)) in r.containers.drain() {
            shim::kill_unit(c.id(), None, KILL_LIMIT);
            let _ = c.kill();
            let _ = c.wait();
        }
        let left = if self.planted(UnitPlant::NoKill) {
            shim::unit_pids(r.child.id(), Some(&r.tag)).len() as u64
        } else if top_only || self.planted(UnitPlant::TagOnly) {
            if top_only {
                let _ = r.child.kill();
                let _ = r.child.wait();
            }
            procs::kill_tagged(&r.tag, if top_only { Duration::ZERO } else { KILL_LIMIT })
        } else {
            shim::kill_unit(r.child.id(), Some(&r.tag), KILL_LIMIT)
        };
        if !self.planted(UnitPlant::NoKill) {
            let _ = r.child.kill();
            let _ = r.child.wait();
        }
        if !self.planted(UnitPlant::KeepFiles) {
            fs::remove_dir_all(&r.root)
                .or_else(|e| if r.root.exists() { Err(e) } else { Ok(()) })
                .map_err(|e| host(e, "remove"))?;
        }
        let files = if r.root.exists() { Count::Known(1) } else { Count::Known(0) };
        if self.planted(UnitPlant::UnknownResidue) {
            return Ok(Residue { processes: Count::Unknown, files: Count::Unknown });
        }
        if shim_lost {
            return Ok(Residue { processes: Count::Unknown, files });
        }
        Ok(Residue { processes: Count::Known(left), files })
    }

    fn observe(&self, unit: &UnitId) -> Result<Observation, UnitError> {
        self.with_unit(unit, |r| {
            let mut pids =
                if self.shimmed() { shim::unit_pids(r.child.id(), Some(&r.tag)) } else { procs::tagged(&r.tag) };
            for (c, _, _) in r.containers.values() {
                pids.extend(shim::unit_pids(c.id(), None));
            }
            // The top process first: the child of the shim, or the child.
            let top = if self.shimmed() { shim::children(r.child.id()).first().copied() } else { Some(r.child.id()) };
            if let Some(i) = pids.iter().position(|p| Some(*p) == top) {
                pids.swap(0, i);
            }
            Ok(Observation { unit_root: Some(r.root.clone()), runner_root: Some(r.runner_root.clone()), pids })
        })
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
