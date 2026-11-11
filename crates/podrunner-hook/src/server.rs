//! The control socket of one unit (`ctl`, GitHub #35): the relay script
//! posts each hook call to `POST /v1/hook/COMMAND`, and a job hook of the
//! runner posts `POST /v1/job_state`. The socket is in the unit root at mode
//! 0600; the uid of the unit is the uid of the runner.

use std::io::BufReader;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Deserialize;

use crate::http::{HttpError, Request, read_request, write_response};
use crate::input::{Command, HookInput, parse};
use crate::policy::Refusal;

/// The header that carries the exit code of a step to the relay.
pub const EXIT_HEADER: &str = "X-Podrunner-Exit";

/// A route of the socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Hook(Command),
    JobState,
}

/// The route of a request: `POST` only.
pub fn route(req: &Request) -> Result<Route, HttpError> {
    if req.method != "POST" {
        return Err(HttpError(405, "POST only"));
    }
    match req.path.as_str() {
        "/v1/job_state" => Ok(Route::JobState),
        p => {
            p.strip_prefix("/v1/hook/").and_then(Command::parse).map(Route::Hook).ok_or(HttpError(404, "no such route"))
        }
    }
}

/// An event of a job hook of the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobEvent {
    JobStarted,
    JobCompleted,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JobState {
    event: JobEvent,
}

/// The answer of a hook call: the exit code of the hook script, and the
/// text it writes (the response file of `prepare_job`, the output of a
/// step).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Answer {
    pub exit: i32,
    pub output: Vec<u8>,
}

/// The unit behind the socket.
pub trait Unit: Send {
    fn hook(&mut self, input: HookInput) -> Result<Answer, Refusal>;
    fn job_state(&mut self, event: JobEvent) -> Result<(), Refusal>;
}

fn refused(r: &Refusal) -> (u16, Vec<(&'static str, String)>, Vec<u8>) {
    let body = serde_json::json!({ "code": r.code.as_str(), "why": r.why });
    (403, vec![("Content-Type", "application/json".into())], body.to_string().into_bytes())
}

fn bad(e: HttpError) -> (u16, Vec<(&'static str, String)>, Vec<u8>) {
    (e.0, vec![], format!("{}\n", e.1).into_bytes())
}

/// The response to one request.
pub fn answer(req: &Request, unit: &mut dyn Unit) -> (u16, Vec<(&'static str, String)>, Vec<u8>) {
    match route(req) {
        Err(e) => bad(e),
        Ok(Route::JobState) => match serde_json::from_slice::<JobState>(&req.body) {
            Err(_) => bad(HttpError(400, "not a job state")),
            Ok(s) => match unit.job_state(s.event) {
                Ok(()) => (200, vec![], Vec::new()),
                Err(r) => refused(&r),
            },
        },
        Ok(Route::Hook(command)) => match parse(command, &req.body) {
            Err(e) => bad(HttpError(400, e.0)),
            Ok(input) => match unit.hook(input) {
                Ok(a) => (200, vec![(EXIT_HEADER, a.exit.to_string())], a.output),
                Err(r) => refused(&r),
            },
        },
    }
}

/// The bytes of `sun_path` on Linux, with its final NUL.
const SUN_PATH: usize = 108;

/// `path`, or a short name of it through an open directory when `path` is
/// too long for `sun_path` (the rootfs of a podvm unit can be): the
/// directory stays open while the name is in use.
fn short(path: &Path) -> std::io::Result<(Option<std::fs::File>, std::path::PathBuf)> {
    if path.as_os_str().len() < SUN_PATH {
        return Ok((None, path.to_path_buf()));
    }
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "a socket path with no name"));
    };
    let d = std::fs::File::open(dir)?;
    use std::os::fd::AsRawFd;
    let link = Path::new("/proc/self/fd").join(d.as_raw_fd().to_string()).join(name);
    Ok((Some(d), link))
}

/// Bind the socket of a unit at mode 0600. A path longer than `sun_path`
/// is bound through its directory.
pub fn bind(path: &Path) -> std::io::Result<UnixListener> {
    let _ = std::fs::remove_file(path);
    let (_dir, name) = short(path)?;
    let l = UnixListener::bind(&name)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(l)
}

/// Connect to the socket at `path`, also when it is longer than
/// `sun_path`.
pub fn connect(path: &Path) -> std::io::Result<std::os::unix::net::UnixStream> {
    let (_dir, name) = short(path)?;
    std::os::unix::net::UnixStream::connect(name)
}

/// Serve the socket until the listener fails: one request for each
/// connection, one connection at a time, so the calls keep their order.
pub fn serve(listener: UnixListener, unit: Arc<Mutex<dyn Unit>>) {
    serve_until(listener, unit, Arc::default());
}

/// Serve until `stop` is set: a connection after it ends the loop.
pub fn serve_until(listener: UnixListener, unit: Arc<Mutex<dyn Unit>>, stop: Arc<AtomicBool>) {
    for conn in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = conn else { return };
        let Ok(mut w) = stream.try_clone() else { continue };
        let (status, headers, body) = match read_request(&mut BufReader::new(stream)) {
            Err(e) => bad(e),
            Ok(req) => match unit.lock() {
                Ok(mut u) => answer(&req, &mut *u),
                Err(_) => bad(HttpError(500, "the unit state is lost")),
            },
        };
        let _ = write_response(&mut w, status, &headers, &body);
    }
}
