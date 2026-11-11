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

use podrunner_schema::{Capability, ContainerRoute, TreeKill, UnitId};
use podrunner_unit::{
    ContainerId, ContainerSpec, ContainerState, Count, ExecSpec, Exit, Observation, Prepared, Residue, Signal,
    StartSpec, UnitCode, UnitDriver, UnitError, parse_pinned,
};

use podrunner_unit::procs::{self, TAG_VAR};

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
}

struct Running {
    root: PathBuf,
    runner_root: PathBuf,
    tag: String,
    child: Child,
    exit: Option<Exit>,
    containers: HashMap<ContainerId, (Child, PathBuf)>,
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
}

impl Drop for FakeUnit {
    /// The plants leave processes behind; a fake leaves none on the host.
    fn drop(&mut self) {
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
        }
    }

    pub fn with_plant(mut self, plant: UnitPlant) -> FakeUnit {
        if plant == UnitPlant::FalseTreeKill {
            self.capability.tree_kill = TreeKill::Verified;
        }
        self.plant = Some(plant);
        self
    }

    fn planted(&self, p: UnitPlant) -> bool {
        self.plant == Some(p)
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
        cmd.spawn().map_err(|e| UnitError::new(UnitCode::UnitStartFailed, format!("spawn: {}", e.kind())))
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
        let child = self.spawn(&spec.argv, &root, &env, None)?;
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
        for (_, (mut c, _)) in r.containers.drain() {
            let _ = c.kill();
            let _ = c.wait();
        }
        let left = if self.planted(UnitPlant::NoKill) {
            procs::tagged(&r.tag).len() as u64
        } else if top_only {
            let _ = r.child.kill();
            let _ = r.child.wait();
            procs::tagged(&r.tag).len() as u64
        } else {
            procs::kill_tagged(&r.tag, KILL_LIMIT)
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
        Ok(Residue { processes: Count::Known(left), files })
    }

    fn observe(&self, unit: &UnitId) -> Result<Observation, UnitError> {
        self.with_unit(unit, |r| {
            let top = r.child.id();
            let mut pids = procs::tagged(&r.tag);
            // The top process first.
            if let Some(i) = pids.iter().position(|p| *p == top) {
                pids.swap(0, i);
            }
            Ok(Observation { unit_root: Some(r.root.clone()), runner_root: Some(r.runner_root.clone()), pids })
        })
    }

    fn container_start(&self, unit: &UnitId, spec: &ContainerSpec) -> Result<ContainerId, UnitError> {
        if self.capability.container != ContainerRoute::Hooks && !self.planted(UnitPlant::ContainerWithNoRoute) {
            return Err(UnitError::new(UnitCode::ContainerNotSupported, "the fake unit has no container route"));
        }
        parse_pinned(&spec.image)?;
        let n = self.containers.fetch_add(1, Ordering::Relaxed);
        let id = ContainerId(format!("c-{n}"));
        let (root, tag) = self.with_unit(unit, |r| Ok((r.root.clone(), r.tag.clone())))?;
        let dir = root.join("containers").join(&id.0);
        fs::create_dir_all(&dir).map_err(|e| host(e, "a container directory"))?;
        // The fake applies no mount and no option: it records them, one on
        // each line, for the tests of the hooks.
        let record: String = spec
            .mounts
            .iter()
            .map(|m| format!("mount {} {} {}\n", m.source.display(), m.target, if m.read_only { "ro" } else { "rw" }))
            .chain(spec.options.iter().map(|(k, v)| format!("option {k} {v}\n")))
            .collect();
        fs::write(dir.join("spec"), record).map_err(|e| host(e, "the record of a container"))?;
        let mut env: Vec<(String, String)> = spec.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        env.push((TAG_VAR.into(), tag));
        let child = self.spawn(&spec.argv, &dir, &env, Some(&dir.join("log")))?;
        self.with_unit(unit, |r| {
            r.containers.insert(id.clone(), (child, dir));
            Ok(id)
        })
    }

    fn container_exec(&self, unit: &UnitId, container: &ContainerId, spec: &ExecSpec) -> Result<Exit, UnitError> {
        if spec.stdin && !self.planted(UnitPlant::AcceptStdin) {
            return Err(UnitError::new(UnitCode::ContainerOptionRefused, "stdin"));
        }
        let (dir, tag) = self.with_unit(unit, |r| {
            let (_, dir) = r
                .containers
                .get(container)
                .ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, container.0.clone()))?;
            Ok((dir.clone(), r.tag.clone()))
        })?;
        let mut env: Vec<(String, String)> = spec.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        env.push((TAG_VAR.into(), tag));
        let mut child = self.spawn(&spec.argv, &dir, &env, spec.log.as_deref())?;
        child.wait().map(procs::exit_of).map_err(|e| host(e, "exec"))
    }

    fn container_logs(&self, unit: &UnitId, container: &ContainerId) -> Result<Vec<u8>, UnitError> {
        self.with_unit(unit, |r| {
            let (_, dir) = r
                .containers
                .get(container)
                .ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, container.0.clone()))?;
            fs::read(dir.join("log")).map_err(|e| host(e, "the log"))
        })
    }

    fn container_inspect(&self, unit: &UnitId, container: &ContainerId) -> Result<ContainerState, UnitError> {
        self.with_unit(unit, |r| {
            let (c, _) = r
                .containers
                .get_mut(container)
                .ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, container.0.clone()))?;
            let exit = c.try_wait().map_err(|e| host(e, "inspect"))?.map(procs::exit_of);
            Ok(ContainerState { running: exit.is_none(), exit })
        })
    }

    fn container_stop(&self, unit: &UnitId, container: &ContainerId) -> Result<(), UnitError> {
        self.with_unit(unit, |r| {
            let (mut c, _) = r
                .containers
                .remove(container)
                .ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, container.0.clone()))?;
            let _ = c.kill();
            c.wait().map(|_| ()).map_err(|e| host(e, "stop"))
        })
    }
}
