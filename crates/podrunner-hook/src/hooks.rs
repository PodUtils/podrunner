//! The control sockets of the units of a host: one socket and one thread
//! for each unit with `container: hooks`, from its start to its destroy.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use podrunner_schema::RefusalCode;
use podrunner_unit::{UnitDriver, UnitId};

use crate::policy::Policy;
use crate::server::{JobEvent, Unit, bind, serve_until};
use crate::unit::{HookUnit, UnitCa};

/// The CA bundle of the broker of a unit, for each container of its job
/// (T-578).
#[derive(Debug, Clone, Copy)]
pub struct Bundle<'a> {
    /// The name of the bundle among the credential files of the unit: the
    /// containers see the copy under the same name.
    pub file: &'a str,
    /// The bundle (PEM).
    pub pem: &'a str,
    /// The variables that name it (`ca::env` of `pod-broker`).
    pub vars: &'a [String],
}

struct Open {
    unit: Arc<Mutex<HookUnit>>,
    stop: Arc<AtomicBool>,
    socket: PathBuf,
    logs: PathBuf,
    /// Gets a message when the thread ends.
    done: std::sync::mpsc::Receiver<()>,
}

/// The longest wait of `close` for the thread of a socket.
pub const CLOSE_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// The open sockets, by unit id.
#[derive(Default)]
pub struct Hooks {
    open: Mutex<HashMap<String, Open>>,
}

impl Hooks {
    /// Serve the socket of `unit` at `socket`. `logs` holds the output of
    /// each step for a short time. `ca`: the CA bundle of the broker of the
    /// unit; each container of the job gets it read-only from `logs/ca`
    /// (T-578).
    pub fn open(
        &self,
        driver: Arc<dyn UnitDriver>,
        unit: &UnitId,
        policy: Policy,
        socket: &Path,
        logs: &Path,
        ca: Option<Bundle<'_>>,
    ) -> std::io::Result<()> {
        std::fs::create_dir_all(logs)?;
        let opened = self.serve(driver, unit, policy, socket, logs, ca);
        if opened.is_err() {
            // No socket serves the unit: no container takes the copy.
            let _ = std::fs::remove_dir_all(logs.join("ca"));
            let _ = std::fs::remove_dir(logs);
        }
        opened
    }

    fn serve(
        &self,
        driver: Arc<dyn UnitDriver>,
        unit: &UnitId,
        policy: Policy,
        socket: &Path,
        logs: &Path,
        ca: Option<Bundle<'_>>,
    ) -> std::io::Result<()> {
        let ca = ca.map(|b| place_ca(logs, b)).transpose()?;
        let listener = bind(socket)?;
        let hook = HookUnit::new(driver, unit.clone(), policy, None, logs.to_path_buf()).with_ca(ca);
        let hook = Arc::new(Mutex::new(hook));
        let stop: Arc<AtomicBool> = Arc::default();
        let (u, s): (Arc<Mutex<dyn Unit>>, _) = (hook.clone(), stop.clone());
        let (end, done) = std::sync::mpsc::channel();
        let _thread = std::thread::Builder::new().name(format!("podrunner-ctl-{unit}")).spawn(move || {
            serve_until(listener, u, s);
            let _ = end.send(());
        })?;
        let open = Open { unit: hook, stop, socket: socket.to_path_buf(), logs: logs.to_path_buf(), done };
        self.open.lock().expect("the sockets").insert(unit.to_string(), open);
        Ok(())
    }

    /// The refusals and the job events of `unit` since the last call.
    pub fn take(&self, unit: &UnitId) -> (Vec<RefusalCode>, Vec<JobEvent>) {
        let open = self.open.lock().expect("the sockets");
        let Some(o) = open.get(unit.as_str()) else { return Default::default() };
        // A step holds the unit for its whole run: the supervisor does not
        // wait for it, and takes the events at a later tick (T-192).
        let Ok(mut u) = o.unit.try_lock() else { return Default::default() };
        // The runner gets the code only; the log of `serve` names the cause
        // of each refusal of the driver (T-177).
        for e in std::mem::take(&mut u.driver_errors) {
            eprintln!("podrunner: {unit}: a hook call was refused: the driver: {e}");
        }
        (std::mem::take(&mut u.refusals), std::mem::take(&mut u.events))
    }

    /// The digests of the images that the containers of `unit` started
    /// with since the last call.
    pub fn take_images(&self, unit: &UnitId) -> Vec<String> {
        let open = self.open.lock().expect("the sockets");
        let Some(o) = open.get(unit.as_str()) else { return Vec::new() };
        o.unit.try_lock().map(|mut u| std::mem::take(&mut u.images)).unwrap_or_default()
    }

    /// End the socket of `unit`: its thread ends, and the socket file goes.
    pub fn close(&self, unit: &UnitId) {
        let Some(o) = self.open.lock().expect("the sockets").remove(unit.as_str()) else { return };
        o.stop.store(true, Ordering::SeqCst);
        // A connection wakes the accept, and the loop sees the stop.
        let _ = crate::server::connect(&o.socket);
        // The thread can be in a call of the driver that does not end (a
        // unit that failed): the supervisor waits for it a short time only,
        // and the thread ends at the end of that call (T-192).
        if o.done.recv_timeout(CLOSE_WAIT).is_err() {
            eprintln!("podrunner: {unit}: the hook thread did not end in {CLOSE_WAIT:?}: it is left");
        }
        let _ = std::fs::remove_file(&o.socket);
        // The logs of the steps are gone; a step that still runs keeps its
        // log, and the directory with it.
        let _ = std::fs::remove_dir_all(o.logs.join("ca"));
        let _ = std::fs::remove_dir(&o.logs);
    }
}

/// Write the bundle `b` to `logs/ca` for the containers of a unit. A
/// certificate is no secret: mode 0644, so that a container user of
/// another uid reads it. `podrunner run` writes its bundle with it too
/// (T-596).
pub fn place_ca(logs: &Path, b: Bundle<'_>) -> std::io::Result<UnitCa> {
    use std::os::unix::fs::PermissionsExt;
    if b.file.is_empty() || b.file.contains('/') || b.file == "." || b.file == ".." {
        let why = format!("the name of the CA bundle {:?} is not a file name", b.file);
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, why));
    }
    let dir = logs.join("ca");
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))?;
    pod_core::files::replace_private(&dir.join(b.file), b.pem.as_bytes(), 0o644, false)?;
    Ok(UnitCa { dir, file: b.file.into(), vars: b.vars.to_vec() })
}
