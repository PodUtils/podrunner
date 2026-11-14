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
//! unit, a `disk` device of the instance.
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
//! A system container shares the kernel of the host: the kind claims
//! `confined`, the floor of `trusted`, never `guest`.

mod guest;

use std::collections::HashMap;
use std::fs;
use std::io::Write;
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

pub use guest::{CHANNEL_PORT_VAR, EGRESS_PORT, SCRIPT, WORK_MOUNT, credential_stream, instance_name, run_args};

/// The time between two reads of the state of an instance.
const POLL: Duration = Duration::from_millis(500);

/// The instance of one unit.
struct Running {
    dir: PathBuf,
    name: String,
    /// `signal(Kill)` removed the instance: its end is that signal.
    killed: bool,
    exit: Option<Exit>,
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
}

fn host(what: &str, e: std::io::Error) -> UnitError {
    UnitError::new(UnitCode::Host, format!("{what}: {}", e.kind()))
}

/// The last bytes of the output of a failed `podvm` call.
fn tail(b: &[u8]) -> String {
    let s = String::from_utf8_lossy(b);
    let s = s.trim();
    s[s.len().saturating_sub(400)..].to_string()
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
        }
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

    /// The state of the instance: `None` when incusd has no such instance.
    fn state(&self, name: &str) -> Option<(String, Option<i32>)> {
        let out = self.verb(&["inspect", "--format", "{{.Status}} {{.ExitCode}}"], name).ok()?;
        let text = String::from_utf8_lossy(&out);
        let mut words = text.split_whitespace();
        let status = words.next()?.to_string();
        Some((status, words.next().and_then(|c| c.parse().ok())))
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

    fn probe(&self) -> Result<Capability, UnitError> {
        Ok(Capability {
            schema: 1,
            kind: "incus".into(),
            arch: std::env::consts::ARCH.into(),
            // A system container: its own namespaces and root, the kernel
            // of the host.
            boundary: Boundary::Confined,
            boundary_proof: Some("incus system container".into()),
            principal: Principal::Distinct,
            container: ContainerRoute::None,
            // No NIC: the channel to the egress socket is the only way out
            // (T-109).
            net: "none".into(),
            secret_delivery: vec![podrunner_schema::SecretDelivery::Files],
            credential_store: CredentialStore::PrivateDir,
            mount_modes: vec!["rw".into()],
            rootfs_fresh: Fact::Yes,
            tree_kill: TreeKill::Unverified,
            limits: Default::default(),
            // No NIC: the channel to the egress socket is the only way out
            // (T-109, `tests/egress.rs`).
            egress_hold: EgressHold::Enforced,
            kvm: Fact::Unknown,
        })
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
        let mut units = self.units.lock().expect("the units");
        let dir = self.unit_dir(&spec.unit);
        if units.contains_key(&spec.unit) || dir.exists() {
            return Err(UnitError::new(UnitCode::UnitInUse, spec.unit.to_string()));
        }
        let reference = self
            .references
            .lock()
            .expect("the references")
            .get(&spec.image.digest)
            .cloned()
            .ok_or_else(|| UnitError::new(UnitCode::UnitStartFailed, "the image is not prepared"))?;
        let stream = credential_stream(&spec.credential)?;
        let work = dir.join("work");
        make_work(&dir, &work)?;
        let name = self.name(&spec.unit);
        let fail = |why: String| {
            let _ = self.verb(&["rm", "-f"], &name);
            let _ = fs::remove_dir_all(&dir);
            UnitError::new(UnitCode::UnitStartFailed, why)
        };
        let egress = spec.sockets.as_ref().map(|s| s.join(podrunner_unit::EGRESS_SOCKET));
        // podvm mounts the socket file into the instance, whose root is a
        // mapped id: the file must take its connect. The socket directory
        // of the unit (mode 0700) keeps each other user of the host out.
        if let Some(e) = egress.as_deref().filter(|e| e.exists()) {
            use std::os::unix::fs::PermissionsExt;
            if let Err(why) = fs::set_permissions(e, fs::Permissions::from_mode(0o666)) {
                let _ = fs::remove_dir_all(&dir);
                return Err(host("the egress socket", why));
            }
        }
        let args = run_args(&name, &reference, &work, egress.as_deref(), &self.cred_root, &spec.env, &spec.argv);
        let mut child =
            match self.podvm().args(&args).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn() {
                Ok(c) => c,
                Err(e) => return Err(fail(format!("podvm run: {}", e.kind()))),
            };
        // `run -d` reads all of stdin before it makes the instance.
        let wrote = child.stdin.take().map(|mut i| i.write_all(&stream));
        let out = child.wait_with_output().map_err(|e| fail(format!("podvm run: {}", e.kind())))?;
        if !out.status.success() || !matches!(wrote, Some(Ok(()))) {
            return Err(fail(format!("podvm run -d: {}", tail(&out.stderr))));
        }
        units.insert(spec.unit.clone(), Running { dir, name, killed: false, exit: None });
        Ok(())
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
            let exit = match self.state(&name) {
                Some((status, _)) if status == "Running" => None,
                Some((_, code)) => Some(Exit::Code(code.unwrap_or(-1))),
                None if killed => Some(Exit::Signal(9)),
                None => Some(Exit::Code(-1)),
            };
            if exit.is_some() {
                let mut units = self.units.lock().expect("the units");
                if let Some(r) = units.get_mut(unit) {
                    r.exit = exit;
                }
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
        let (dir, name) = match units.remove(unit) {
            Some(r) => (r.dir, r.name),
            // A second call, or a replay of the journal: the name is known.
            None => (self.unit_dir(unit), self.name(unit)),
        };
        let left = self.remove(&name);
        let removed = fs::remove_dir_all(&dir).is_ok() || !dir.exists();
        Ok(Residue { processes: Count::Known(left), files: Count::Known(u64::from(!removed)) })
    }

    fn observe(&self, unit: &UnitId) -> Result<Observation, UnitError> {
        let units = self.units.lock().expect("the units");
        let r = units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
        // No process of the instance is a child of podrunner.
        Ok(Observation { unit_root: Some(r.dir.join("work")), runner_root: None, pids: Vec::new() })
    }

    fn unit_logs(&self, unit: &UnitId) -> Result<Vec<u8>, UnitError> {
        let name = {
            let units = self.units.lock().expect("the units");
            units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?.name.clone()
        };
        self.verb(&["logs"], &name).map_err(|e| UnitError::new(UnitCode::Host, format!("podvm logs: {e}")))
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
    fn container_inspect(&self, _: &UnitId, _: &ContainerId) -> Result<ContainerState, UnitError> {
        Err(no_container())
    }
    fn container_stop(&self, _: &UnitId, _: &ContainerId) -> Result<(), UnitError> {
        Err(no_container())
    }
}

/// Make the work directory: mode 0777, so the mapped user of the instance
/// writes in it. Its parent, the unit directory, is 0700: no other user of
/// the host reaches it.
fn make_work(unit_dir: &Path, work: &Path) -> Result<(), UnitError> {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(work).map_err(|e| host("the work directory", e))?;
    fs::set_permissions(unit_dir, fs::Permissions::from_mode(0o700)).map_err(|e| host("the work directory", e))?;
    fs::set_permissions(work, fs::Permissions::from_mode(0o777)).map_err(|e| host("the work directory", e))
}

fn no_container() -> UnitError {
    UnitError::new(
        UnitCode::ContainerNotSupported,
        "the incus kind has no container route yet: the engine of T-103 comes later",
    )
}

#[cfg(test)]
mod tests;
