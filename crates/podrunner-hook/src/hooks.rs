//! The control sockets of the units of a host: one socket and one thread
//! for each unit with `container: hooks`, from its start to its destroy.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use podrunner_schema::RefusalCode;
use podrunner_unit::{UnitDriver, UnitId};

use crate::policy::Policy;
use crate::server::{JobEvent, Unit, bind, serve_until};
use crate::unit::HookUnit;

struct Open {
    unit: Arc<Mutex<HookUnit>>,
    stop: Arc<AtomicBool>,
    socket: PathBuf,
    thread: JoinHandle<()>,
}

/// The open sockets, by unit id.
#[derive(Default)]
pub struct Hooks {
    open: Mutex<HashMap<String, Open>>,
}

impl Hooks {
    /// Serve the socket of `unit` at `socket`. `logs` holds the output of
    /// each step for a short time.
    pub fn open(
        &self,
        driver: Arc<dyn UnitDriver>,
        unit: &UnitId,
        policy: Policy,
        socket: &Path,
        logs: &Path,
    ) -> std::io::Result<()> {
        std::fs::create_dir_all(logs)?;
        let listener = bind(socket)?;
        let hook = Arc::new(Mutex::new(HookUnit::new(driver, unit.clone(), policy, None, logs.to_path_buf())));
        let stop: Arc<AtomicBool> = Arc::default();
        let (u, s): (Arc<Mutex<dyn Unit>>, _) = (hook.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name(format!("podrunner-ctl-{unit}"))
            .spawn(move || serve_until(listener, u, s))?;
        let open = Open { unit: hook, stop, socket: socket.to_path_buf(), thread };
        self.open.lock().expect("the sockets").insert(unit.to_string(), open);
        Ok(())
    }

    /// The refusals and the job events of `unit` since the last call.
    pub fn take(&self, unit: &UnitId) -> (Vec<RefusalCode>, Vec<JobEvent>) {
        let open = self.open.lock().expect("the sockets");
        let Some(o) = open.get(unit.as_str()) else { return Default::default() };
        let Ok(mut u) = o.unit.lock() else { return Default::default() };
        (std::mem::take(&mut u.refusals), std::mem::take(&mut u.events))
    }

    /// End the socket of `unit`: its thread ends, and the socket file goes.
    pub fn close(&self, unit: &UnitId) {
        let Some(o) = self.open.lock().expect("the sockets").remove(unit.as_str()) else { return };
        o.stop.store(true, Ordering::SeqCst);
        // A connection wakes the accept, and the loop sees the stop.
        let _ = std::os::unix::net::UnixStream::connect(&o.socket);
        let _ = o.thread.join();
        let _ = std::fs::remove_file(&o.socket);
    }
}
