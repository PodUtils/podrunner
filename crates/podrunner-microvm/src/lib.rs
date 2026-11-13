//! The microvm unit driver (T-037, GitHub #37): one guest of the machine
//! tier of podvm (`podvm run --podvm-tier=machine`) for each job, from a
//! pinned image of the operator's store.
//!
//! The machine tier runs a guest only in the foreground, so the driver
//! holds the `podvm run` process of each unit. The credential files go to
//! a private directory of the unit on the host, which the guest mounts
//! read-only over 9p; the guest copies them into the runner root at mode
//! 0600 and runs the payload as the user of the image. No credential goes
//! into argv or the environment. The payload runs in a work directory of
//! the unit that the guest mounts over 9p. Each process of the unit (podvm, QEMU)
//! carries the tag of the unit, so `destroy` ends the whole tree. A unix
//! socket does not cross into a guest: the kind has no container route.
//!
//! The guest has no NIC (`--podvm-net none`). With a socket directory, its
//! one way out is a channel of podvm from `127.0.0.1:3128` in the guest to
//! the egress socket of the unit, and the payload gets that address as its
//! proxy: the hold needs no help of the host.

mod guest;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use podrunner_podvm::Podvm;
use podrunner_schema::{
    Boundary, Capability, ContainerRoute, CredentialStore, EgressHold, Fact, Principal, TreeKill, UnitId,
};
use podrunner_unit::procs::{self, TAG_VAR};
use podrunner_unit::{
    ContainerId, ContainerSpec, ContainerState, Count, EGRESS_SOCKET, ExecSpec, Exit, Observation, Prepared, Residue,
    Signal, StartSpec, UnitCode, UnitDriver, UnitError, parse_pinned,
};

pub use guest::{CRED_MOUNT, EGRESS_PORT, Shape, WORK_MOUNT, proxy_env, run_args};

/// The time that `destroy` waits for the processes of a unit to end.
const KILL_LIMIT: Duration = Duration::from_secs(10);

/// The guest of one unit.
struct Running {
    dir: PathBuf,
    tag: String,
    child: Child,
    exit: Option<Exit>,
}

/// The microvm unit driver.
pub struct MicrovmUnit {
    podvm: Podvm,
    state_dir: PathBuf,
    /// The store of the operator that holds the pinned images.
    images: PathBuf,
    /// The guest memory (`--podvm-mem`): the image is an initramfs, which
    /// must fit twice (packed and unpacked).
    mem: String,
    cpus: u32,
    platform: String,
    /// The reference of each prepared digest.
    references: Mutex<HashMap<String, String>>,
    units: Mutex<HashMap<UnitId, Running>>,
    /// Where the guest writes the credential files: the runner image links
    /// `.runner`, `.credentials` and `.credentials_rsaparams` of
    /// `/home/runner` to it, as for the podvm kind.
    cred_root: String,
    /// A directory that keeps the log of each guest after `destroy`, for a
    /// test that reads it then.
    kept_logs: Option<PathBuf>,
}

fn host(what: &str, e: std::io::Error) -> UnitError {
    UnitError::new(UnitCode::Host, format!("{what}: {}", e.kind()))
}

impl MicrovmUnit {
    /// A driver with its units under `state_dir` and the pinned images in
    /// the podvm store `images`.
    pub fn new(podvm: Podvm, state_dir: &Path, images: &Path) -> MicrovmUnit {
        let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
        MicrovmUnit {
            podvm,
            state_dir: state_dir.to_path_buf(),
            images: images.to_path_buf(),
            mem: "4G".into(),
            cpus: 2,
            platform: platform.into(),
            references: Mutex::new(HashMap::new()),
            units: Mutex::new(HashMap::new()),
            cred_root: "/podrunner/.runner".into(),
            kept_logs: None,
        }
    }

    /// Keep the log of each guest as `DIR/UNIT.log` at `destroy`.
    pub fn with_kept_logs(mut self, dir: &Path) -> MicrovmUnit {
        self.kept_logs = Some(dir.to_path_buf());
        self
    }

    /// The guest memory and vCPUs of each unit.
    pub fn with_size(mut self, mem: &str, cpus: u32) -> MicrovmUnit {
        self.mem = mem.into();
        self.cpus = cpus;
        self
    }

    fn unit_dir(&self, unit: &UnitId) -> PathBuf {
        self.state_dir.join("units").join(unit.as_str())
    }
}

/// Whether `/dev/kvm` opens for read and write.
fn kvm() -> Fact {
    match fs::OpenOptions::new().read(true).write(true).open("/dev/kvm") {
        Ok(_) => Fact::Yes,
        Err(_) => Fact::No,
    }
}

impl UnitDriver for MicrovmUnit {
    fn kind(&self) -> &str {
        "microvm"
    }

    fn probe(&self) -> Result<Capability, UnitError> {
        Ok(Capability {
            schema: 1,
            kind: "microvm".into(),
            arch: std::env::consts::ARCH.into(),
            // A claim: slots takes it only after the self-test of T-036.
            boundary: Boundary::Guest,
            boundary_proof: None,
            principal: Principal::Distinct,
            container: ContainerRoute::None,
            net: "none".into(),
            secret_delivery: vec![podrunner_schema::SecretDelivery::Files],
            // The files wait on the disk of the host until the guest copies them.
            credential_store: CredentialStore::PrivateDir,
            mount_modes: vec!["ro".into()],
            rootfs_fresh: Fact::Yes,
            tree_kill: TreeKill::Unverified,
            limits: Default::default(),
            // No NIC: the channel to the egress socket is the only way out.
            egress_hold: EgressHold::Enforced,
            kvm: kvm(),
        })
    }

    fn prepare(&self, reference: &str, platform: &str, _emulation: bool) -> Result<Prepared, UnitError> {
        let (_, digest) = parse_pinned(reference)?;
        if platform != self.platform {
            return Err(UnitError::new(
                UnitCode::ArchMismatch,
                format!("{platform} on {}: the guest emulates nothing", self.platform),
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
        let (cred, work) = (dir.join("cred"), dir.join("work"));
        if let Err(e) = guest::make_work(&dir, &work).and_then(|()| guest::write_credentials(&cred, &spec.credential)) {
            let _ = fs::remove_dir_all(&dir);
            return Err(e);
        }
        let tag = procs::tag_of(&self.state_dir, spec.unit.as_str());
        let egress = spec.sockets.as_ref().map(|s| s.join(EGRESS_SOCKET));
        let shape = Shape { mem: &self.mem, cpus: self.cpus, egress: egress.as_deref() };
        let args = run_args(&reference, &dir, &shape, &self.cred_root, &spec.env, &spec.argv);
        // The output of the guest: `unit_logs` reads it before `destroy`.
        let log = fs::File::create(dir.join("guest.log")).map_err(|e| host("the log of the guest", e))?;
        let log2 = log.try_clone().map_err(|e| host("the log of the guest", e))?;
        let mut cmd = Command::new(&self.podvm.binary);
        // No file that podvm or QEMU makes for the unit (the socket of the
        // channel too) is open to another user of the host.
        // SAFETY: umask is async-signal-safe and changes only the child.
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| {
                libc::umask(0o077);
                Ok(())
            });
        }
        let child = cmd
            .args(&args)
            .env("PODVM_STORE", &self.images)
            .env(TAG_VAR, &tag)
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(log2)
            .spawn()
            .map_err(|e| {
                let _ = fs::remove_dir_all(&dir);
                host("podvm run", e)
            })?;
        units.insert(spec.unit.clone(), Running { dir, tag, child, exit: None });
        Ok(())
    }

    fn signal(&self, unit: &UnitId, signal: Signal) -> Result<(), UnitError> {
        let units = self.units.lock().expect("the units");
        let r = units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
        let sig = match signal {
            Signal::Term => libc::SIGTERM,
            Signal::Kill => libc::SIGKILL,
        };
        for pid in procs::tagged(&r.tag) {
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
                if r.exit.is_none()
                    && let Some(status) = r.child.try_wait().map_err(|e| host("podvm run", e))?
                {
                    use std::os::unix::process::ExitStatusExt;
                    r.exit = Some(match (status.code(), status.signal()) {
                        (Some(c), _) => Exit::Code(c),
                        (None, Some(s)) => Exit::Signal(s),
                        _ => Exit::Code(-1),
                    });
                }
                if r.exit.is_some() || Instant::now() >= end {
                    return Ok(r.exit);
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn destroy(&self, unit: &UnitId) -> Result<Residue, UnitError> {
        let mut units = self.units.lock().expect("the units");
        let Some(mut r) = units.remove(unit) else {
            // A second call counts again: nothing of this unit is known.
            let dir = self.unit_dir(unit);
            let files = if dir.exists() { Count::Known(1) } else { Count::Known(0) };
            let tag = procs::tag_of(&self.state_dir, unit.as_str());
            return Ok(Residue { processes: Count::Known(procs::tagged(&tag).len() as u64), files });
        };
        let left = procs::kill_tagged(&r.tag, KILL_LIMIT);
        let _ = r.child.kill();
        let _ = r.child.wait();
        if let Some(keep) = &self.kept_logs {
            let _ = fs::create_dir_all(keep);
            let _ = fs::copy(r.dir.join("guest.log"), keep.join(format!("{unit}.log")));
        }
        let removed = fs::remove_dir_all(&r.dir).is_ok() || !r.dir.exists();
        Ok(Residue { processes: Count::Known(left), files: Count::Known(u64::from(!removed)) })
    }

    fn observe(&self, unit: &UnitId) -> Result<Observation, UnitError> {
        let units = self.units.lock().expect("the units");
        let r = units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
        Ok(Observation {
            unit_root: Some(r.dir.join("work")),
            runner_root: Some(r.dir.join("cred")),
            pids: procs::tagged(&r.tag),
        })
    }

    fn unit_logs(&self, unit: &UnitId) -> Result<Vec<u8>, UnitError> {
        let units = self.units.lock().expect("the units");
        let r = units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
        fs::read(r.dir.join("guest.log")).map_err(|e| host("the log of the guest", e))
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

fn no_container() -> UnitError {
    UnitError::new(
        UnitCode::ContainerNotSupported,
        "the microvm kind has no container route: a socket does not cross into a guest",
    )
}

#[cfg(test)]
mod tests;
