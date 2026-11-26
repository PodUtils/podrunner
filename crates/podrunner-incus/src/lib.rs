//! The incus unit driver (T-105, githome's G-02): one system container of
//! the incus tier of podvm (`podvm run -d -i --podvm-tier=incus`) for each
//! job, from a pinned image of the operator's store.
//!
//! The instance has a name that the driver chooses, so `destroy` reaches
//! it with `podvm rm -f` also when no `podvm` process of the unit is left:
//! the processes of the instance belong to incusd and carry no tag of the
//! unit. The credential files go in on stdin (`/.podvm/stdin`, owner 0,
//! mode 0600, removed once read): the root of the instance is a mapped id
//! and cannot read a private directory of the host. No credential goes into
//! argv or the environment. The payload runs in a work directory of the
//! unit, a `disk` device of the instance. The cache of the job is the one
//! host directory of a start (`cache`, T-648).
//!
//! The instance has no NIC (`--podvm-net none`). Its one way out is a
//! channel of podvm (`--podvm-channel`, no incus `proxy` device) from
//! `127.0.0.1:3128` in the instance to the egress socket of the unit:
//! podvm mounts the socket file into the instance and relays to it. The
//! driver sets the socket to mode 0666, so the mapped root of the instance
//! can connect; the socket directory of the unit (mode 0700) keeps each
//! other user of the host out. The script sets the proxy variables of the
//! payload from `PODRUNNER_CHANNEL_PORT` (T-109).
//!
//! A warm instance (T-487, `warm.rs`) starts with no stdin and waits; its
//! claim comes on the stdin of one `podvm exec -i`.
//!
//! A system container shares the kernel of the host: the kind claims
//! `confined`, the floor of `trusted`, never `guest`.

pub mod cache;
mod guest;
mod start;
mod stderr;
mod usage;
mod warm;

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use podrunner_podvm::Podvm;
use podrunner_schema::{
    Boundary, Capability, ContainerRoute, CredentialStore, EgressHold, Fact, Principal, TreeKill, UnitId,
};
use podrunner_unit::{
    ContainerId, ContainerSpec, ContainerState, Count, ExecSpec, Exit, Observation, Prepared, Residue, Signal,
    StartSpec, UnitCode, UnitDriver, UnitError, parse_pinned,
};

use stderr::tail;

pub use guest::{
    CHANNEL_PORT_VAR, EGRESS_PORT, ENGINE_MOUNT, SCRIPT, WORK_MOUNT, credential_stream, instance_name, run_args,
};

/// The time between two reads of the state of an instance.
const POLL: Duration = Duration::from_millis(500);

/// The instance of one unit.
struct Running {
    dir: PathBuf,
    name: String,
    /// `signal(Kill)` removed the instance: its end is that signal.
    killed: bool,
    exit: Option<Exit>,
    /// The use that the reads of the state saw while the instance ran.
    kept: usage::Kept,
    /// A warm unit (T-487): claimed or not; `None` for a unit of `start`.
    claimed: Option<bool>,
}

/// The incus unit driver.
pub struct IncusUnit {
    podvm: Podvm,
    state_dir: PathBuf,
    /// The store of the operator that holds the pinned images.
    images: PathBuf,
    platform: String,
    /// The reference of each prepared digest.
    references: Mutex<HashMap<String, String>>,
    units: Mutex<HashMap<UnitId, Running>>,
    /// Where the instance writes the credential files: the runner image
    /// links `.runner`, `.credentials` and `.credentials_rsaparams` of
    /// `/home/runner` to it, as for the podvm kind.
    cred_root: String,
    /// The limits of each instance (T-162).
    limits: podrunner_unit::limits::Limits,
    /// The pool of incusd holds a size of a root disk (T-482): each
    /// instance gets one, `disk_bytes` of its limits.
    root_quota: bool,
    /// The directory of the static Docker binaries: each instance runs an
    /// engine of its own (T-485).
    engine: Option<PathBuf>,
    payload_engines: BTreeSet<String>,
    /// The init of the image is PID 1 (`[unit.incus] init = "image"`, T-543).
    init_image: bool,
}

pub(crate) fn host(what: &str, e: std::io::Error) -> UnitError {
    UnitError::new(UnitCode::Host, format!("{what}: {}", e.kind()))
}

impl IncusUnit {
    /// A driver with its units under `state_dir` and the pinned images in
    /// the podvm store `images`. podvm finds incusd by
    /// `PODVM_INCUS_SOCKET` or `INCUS_DIR` of the environment.
    pub fn new(podvm: Podvm, state_dir: &Path, images: &Path) -> IncusUnit {
        let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
        IncusUnit {
            podvm,
            state_dir: state_dir.to_path_buf(),
            images: images.to_path_buf(),
            platform: platform.into(),
            references: Mutex::new(HashMap::new()),
            units: Mutex::new(HashMap::new()),
            cred_root: "/podrunner/.runner".into(),
            limits: Default::default(),
            root_quota: false,
            engine: None,
            payload_engines: BTreeSet::new(),
            init_image: false,
        }
    }

    /// Boot the init of the image as PID 1 of each instance (`[unit.incus]
    /// init = "image"`, T-543): the services and the engine of the image
    /// run, as on the GitHub-hosted runner. The record says `container:
    /// engine`.
    pub fn with_image_init(mut self, on: bool) -> IncusUnit {
        self.init_image = on;
        self
    }

    /// Run a container engine in each instance, from the static Docker
    /// binaries of `dir` (`[unit.incus] engine`, T-485): podvm sets
    /// `security.nesting` and the intercepts of `mknod` and `setxattr`; the
    /// instance stays unprivileged. The record says `container: engine`.
    pub fn with_engine(mut self, dir: &Path) -> IncusUnit {
        self.engine = Some(dir.to_path_buf());
        self
    }

    /// The digests of payloads that carry the guest engine.
    pub fn with_payload_engines(mut self, digests: BTreeSet<String>) -> Self {
        self.payload_engines = digests;
        self
    }

    /// The pool of incusd holds a size of a root disk: each instance gets
    /// `--storage-opt size=` of the lesser of the `disk` of the offer and
    /// of the kind (`[unit.incus] root_quota`, T-482). podvm refuses the
    /// size on a pool with no quota (the `dir` driver).
    pub fn with_root_quota(mut self, on: bool) -> IncusUnit {
        self.root_quota = on;
        self
    }

    /// The limits of memory, CPU and processes of each instance
    /// (`[unit.incus] mem`, `cpus`, `pids`; T-162).
    pub fn with_limits(mut self, limits: podrunner_unit::limits::Limits) -> IncusUnit {
        self.limits = limits;
        self
    }

    fn unit_dir(&self, unit: &UnitId) -> PathBuf {
        self.state_dir.join("units").join(unit.as_str())
    }

    fn name(&self, unit: &UnitId) -> String {
        instance_name(&self.state_dir, unit.as_str())
    }

    fn podvm(&self) -> Command {
        let mut c = Command::new(&self.podvm.binary);
        c.env("PODVM_STORE", &self.images).stdin(Stdio::null());
        c
    }

    /// `podvm VERB --podvm-tier=incus NAME`: its stdout, or why it failed.
    fn verb(&self, verb: &[&str], name: &str) -> Result<Vec<u8>, String> {
        let out = self
            .podvm()
            .args(verb)
            .arg("--podvm-tier=incus")
            .arg(name)
            .output()
            .map_err(|e| format!("podvm {}: {}", verb[0], e.kind()))?;
        if out.status.success() { Ok(out.stdout) } else { Err(tail(&out.stderr)) }
    }

    /// The state of the instance, with its use (T-483): `None` when incusd
    /// has no such instance. The use costs no process of its own: podvm
    /// reads it from incusd on each `inspect`. A podvm before
    /// `0.2.0-beta.33` refuses its fields: then a second read gives the
    /// status, and the use is the reason.
    fn state(&self, name: &str) -> Option<usage::State> {
        let read = |format: &str| self.verb(&["inspect", "--format", format], name).ok();
        match read(usage::STATE) {
            Some(out) => usage::state(&String::from_utf8_lossy(&out), true),
            None => usage::state(&String::from_utf8_lossy(&read(usage::STATUS)?), false),
        }
    }

    /// Remove the instance, and count what is left of it.
    fn remove(&self, name: &str) -> u64 {
        let _ = self.verb(&["rm", "-f"], name);
        u64::from(self.state(name).is_some())
    }
}

impl UnitDriver for IncusUnit {
    fn kind(&self) -> &str {
        "incus"
    }

    fn uses_payload_runner(&self) -> bool {
        true
    }

    fn engine_needs_payload(&self) -> bool {
        self.engine.is_none() && !self.init_image && !self.payload_engines.is_empty()
    }

    fn credential_dir(&self, _unit: &UnitId) -> Option<String> {
        Some(self.cred_root.clone())
    }

    /// The cache of the job only ([`cache`], T-648).
    fn host_mounts(&self) -> bool {
        true
    }

    fn probe(&self) -> Result<Capability, UnitError> {
        let mut cap = Capability {
            schema: 1,
            kind: "incus".into(),
            arch: std::env::consts::ARCH.into(),
            // A system container: its own namespaces and root, the kernel
            // of the host.
            boundary: Boundary::Confined,
            boundary_proof: Some("incus system container".into()),
            principal: Principal::Distinct,
            container: if self.engine.is_some() || self.init_image || self.engine_needs_payload() {
                ContainerRoute::Engine
            } else {
                ContainerRoute::None
            },
            // No NIC: the channel to the egress socket is the only way out
            // (T-109).
            net: "none".into(),
            secret_delivery: vec![podrunner_schema::SecretDelivery::Files],
            credential_store: CredentialStore::PrivateDir,
            // The cache of a job at `ro` only (`cache`, T-648).
            mount_modes: vec!["ro".into()],
            rootfs_fresh: Fact::Yes,
            tree_kill: TreeKill::Unverified,
            limits: Default::default(),
            // No NIC: the channel to the egress socket is the only way out
            // (T-109, `tests/egress.rs`).
            egress_hold: EgressHold::Enforced,
            kvm: Fact::Unknown,
        };
        // The keys of an instance: the kind applies each limit that an offer
        // or its configuration names (T-198).
        podrunner_unit::limits::Limits { memory_bytes: Some(1), cpus: Some(1), pids: Some(1), disk_bytes: None }
            .record(&mut cap);
        // A size of the root disk of each instance, on a pool with quotas
        // (T-482): a job past it gets ENOSPC.
        if self.root_quota {
            cap.limits.insert("disk".into(), podrunner_schema::LimitMode::Enforced);
        }
        Ok(cap)
    }

    fn prepare(&self, reference: &str, platform: &str, _emulation: bool) -> Result<Prepared, UnitError> {
        let (_, digest) = parse_pinned(reference)?;
        if platform != self.platform {
            return Err(UnitError::new(
                UnitCode::ArchMismatch,
                format!("{platform} on {}: a system container emulates nothing", self.platform),
            ));
        }
        self.references.lock().expect("the references").insert(digest.to_string(), reference.to_string());
        Ok(Prepared { digest: digest.to_string(), platform: platform.into() })
    }

    fn start(&self, spec: &StartSpec) -> Result<(), UnitError> {
        self.launch(spec, None)
    }

    fn warm(&self, spec: &podrunner_unit::warm::WarmSpec) -> Result<(), UnitError> {
        self.warm_unit(spec)
    }

    fn warm_ready(&self, unit: &UnitId) -> Result<bool, UnitError> {
        self.warm_ready_of(unit)
    }

    fn claim(&self, unit: &UnitId, spec: &podrunner_unit::warm::ClaimSpec) -> Result<(), UnitError> {
        self.claim_unit(unit, spec)
    }

    fn applied_limits(&self, job: &podrunner_unit::limits::Limits) -> podrunner_unit::limits::Limits {
        self.applied(job)
    }

    fn signal(&self, unit: &UnitId, signal: Signal) -> Result<(), UnitError> {
        let mut units = self.units.lock().expect("the units");
        let r = units.get_mut(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
        match signal {
            // podvm stops the instance with a time of grace.
            Signal::Term => {
                let _ = self.verb(&["stop"], &r.name);
            }
            Signal::Kill => {
                r.killed = true;
                let _ = self.verb(&["rm", "-f"], &r.name);
            }
        }
        Ok(())
    }

    fn wait(&self, unit: &UnitId, timeout: Duration) -> Result<Option<Exit>, UnitError> {
        let end = Instant::now() + timeout;
        loop {
            let (name, killed) = {
                let units = self.units.lock().expect("the units");
                let r = units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
                if r.exit.is_some() {
                    return Ok(r.exit);
                }
                (r.name.clone(), r.killed)
            };
            let state = self.state(&name);
            let exit = match &state {
                Some(s) if s.running() => None,
                Some(s) => Some(Exit::Code(s.code.unwrap_or(-1))),
                None if killed => Some(Exit::Signal(9)),
                None => Some(Exit::Code(-1)),
            };
            // Each read is a sample of the use: incusd gives none after the
            // stop. The read of the stop gives the record of podvm
            // `0.2.0-beta.39` (T-611); with an older podvm the last sample
            // is up to one read (one tick of slots, or `POLL`) before the
            // stop, so `cpu_s` can count that time of each busy CPU less.
            if let Some(r) = self.units.lock().expect("the units").get_mut(unit) {
                state.iter().for_each(|s| r.kept.take(s));
                r.exit = r.exit.or(exit);
            }
            if exit.is_some() {
                return Ok(exit);
            }
            if Instant::now() >= end {
                return Ok(None);
            }
            std::thread::sleep(POLL.min(end.saturating_duration_since(Instant::now())));
        }
    }

    fn destroy(&self, unit: &UnitId) -> Result<Residue, UnitError> {
        let mut units = self.units.lock().expect("the units");
        let (dir, name, replay) = match units.remove(unit) {
            Some(r) => (r.dir, r.name, false),
            // A second call, or a replay of the journal: the name is known.
            None => (self.unit_dir(unit), self.name(unit), true),
        };
        let mut left = self.remove(&name);
        if replay {
            // An instance of a release before #273 (a 32-bit name).
            left += self.remove(&crate::guest::legacy_instance_name(&self.state_dir, unit.as_str()));
        }
        let removed = fs::remove_dir_all(&dir).is_ok() || !dir.exists();
        Ok(Residue { processes: Count::Known(left), files: Count::Known(u64::from(!removed)) })
    }

    fn observe(&self, unit: &UnitId) -> Result<Observation, UnitError> {
        let units = self.units.lock().expect("the units");
        let r = units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
        // No process of the instance is a child of podrunner.
        Ok(Observation { unit_root: Some(r.dir.join("work")), runner_root: None, pids: Vec::new() })
    }

    fn unit_log(&self, unit: &UnitId) -> Result<podrunner_unit::UnitLog, UnitError> {
        let name = {
            let units = self.units.lock().expect("the units");
            units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?.name.clone()
        };
        // A podvm before `0.2.0-beta.38` reads the file of incus whole: 64 MiB at most.
        Ok(self.podvm.logs(&["--podvm-tier=incus"], &name, vec![("PODVM_STORE", self.images.clone().into())]))
    }

    /// `podvm exec --podvm-tier=incus` in the instance (T-160). The
    /// instance has no NIC: the command has the way out of the payload.
    fn enter(&self, unit: &UnitId, argv: &[String], tty: bool) -> Result<Command, UnitError> {
        let name = {
            let units = self.units.lock().expect("the units");
            units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?.name.clone()
        };
        let mut c = Command::new(&self.podvm.binary);
        c.env("PODVM_STORE", &self.images).args(["exec", "-i"]);
        if tty {
            c.arg("-t");
        }
        c.arg("--podvm-tier=incus").arg(name).args(argv);
        Ok(c)
    }

    fn container_start(&self, _: &UnitId, _: &ContainerSpec) -> Result<ContainerId, UnitError> {
        Err(no_container())
    }
    fn container_exec(&self, _: &UnitId, _: &ContainerId, _: &ExecSpec) -> Result<Exit, UnitError> {
        Err(no_container())
    }
    fn container_logs(&self, _: &UnitId, _: &ContainerId) -> Result<Vec<u8>, UnitError> {
        Err(no_container())
    }
    fn usage(&self, unit: &UnitId) -> Option<podrunner_unit::UnitUsage> {
        // The processes of an instance are not on the host: the kind always
        // answers, with a value or a reason. One more read: the instance can
        // still run, or a podvm can keep the use of a stopped one.
        let state = self.state(&self.name(unit));
        let mut units = self.units.lock().expect("the units");
        let Some(r) = units.get_mut(unit) else { return Some(state.map_or_else(usage::failed, |s| s.usage)) };
        state.iter().for_each(|s| r.kept.take(s));
        Some(r.kept.usage())
    }

    fn oom_kills(&self, unit: &UnitId) -> Option<u64> {
        // podvm `0.2.0-beta.33`: `OomKills` of the metrics of incusd, which
        // has none of a stopped instance; `0.2.0-beta.39`: also the record
        // of the stop. The highest count that a read saw.
        let state = self.state(&self.name(unit));
        let mut units = self.units.lock().expect("the units");
        let Some(r) = units.get_mut(unit) else { return state?.oom_kills };
        state.iter().for_each(|s| r.kept.take(s));
        r.kept.oom_kills()
    }

    fn container_inspect(&self, _: &UnitId, _: &ContainerId) -> Result<ContainerState, UnitError> {
        Err(no_container())
    }
    fn container_stop(&self, _: &UnitId, _: &ContainerId) -> Result<(), UnitError> {
        Err(no_container())
    }
}

fn no_container() -> UnitError {
    UnitError::new(
        UnitCode::ContainerNotSupported,
        "the incus kind takes no container hooks: a job reaches containers through the engine in its instance (`engine`)",
    )
}

#[cfg(test)]
mod tests;
