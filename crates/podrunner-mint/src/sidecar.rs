//! A source in a child process: the scale-set sidecar (GitHub #29). `mint`
//! starts it with the source wire on its stdio and a channel to `sign` on
//! its descriptor 3 (call C15), and starts it again with a backoff when it
//! ends, except after a revocation.

use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use podrunner_schema::{Credential, JobId};
use podrunner_source::{Event, Hello, JobSource, SourceCode, SourceError, WireSource};
use serde_json::{Map, Value};

use crate::sign::{Refusal, Signer};

/// The first and the longest wait before a restart.
const BACKOFF_FIRST: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(60);

struct Running {
    child: Child,
    wire: WireSource<BufReader<ChildStdout>, ChildStdin>,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The sidecar as a [`JobSource`].
pub struct SidecarSource {
    program: String,
    args: Vec<String>,
    signer: Arc<Mutex<Signer>>,
    caller: String,
    running: Option<Running>,
    backoff: Duration,
    not_before: Option<Instant>,
    revoked: bool,
    /// The starts of the sidecar, for `status`.
    pub starts: u64,
}

/// Answer the sign requests of one sidecar until its channel closes.
fn serve_sign(stream: UnixStream, signer: Arc<Mutex<Signer>>, caller: String) {
    let Ok(mut w) = stream.try_clone() else { return };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        let req: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        let claims = req.get("claims").and_then(Value::as_object).cloned().unwrap_or_default();
        let reply = match signer.lock().map(|mut s| s.sign(&caller, &claims)) {
            Ok(Ok(token)) => serde_json::json!({ "token": token }),
            Ok(Err(Refusal::Rate)) => serde_json::json!({ "error": "rate" }),
            Ok(Err(Refusal::Shape(_))) => serde_json::json!({ "error": "shape" }),
            _ => serde_json::json!({ "error": "key" }),
        };
        if w.write_all(format!("{reply}\n").as_bytes()).is_err() {
            return;
        }
    }
}

impl SidecarSource {
    pub fn new(program: &str, args: Vec<String>, signer: Arc<Mutex<Signer>>, caller: &str) -> SidecarSource {
        SidecarSource {
            program: program.into(),
            args,
            signer,
            caller: caller.into(),
            running: None,
            backoff: BACKOFF_FIRST,
            not_before: None,
            revoked: false,
            starts: 0,
        }
    }

    fn start(&mut self) -> Result<(), SourceError> {
        if self.not_before.is_some_and(|t| Instant::now() < t) {
            return Err(SourceError::new(SourceCode::Unavailable, "the sidecar waits for its restart"));
        }
        let down =
            |what: &str, e: std::io::Error| SourceError::new(SourceCode::Unavailable, format!("{what}: {}", e.kind()));
        let (ours, theirs) = UnixStream::pair().map_err(|e| down("the sign channel", e))?;
        let fd = theirs.as_raw_fd();
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
        // SAFETY: dup2 in the child before exec, with a descriptor that is
        // open in the parent until the spawn returns.
        unsafe {
            cmd.pre_exec(move || if libc::dup2(fd, 3) < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) });
        }
        let mut child = cmd.spawn().map_err(|e| down("the sidecar", e))?;
        drop(theirs);
        let (signer, caller) = (self.signer.clone(), self.caller.clone());
        std::thread::spawn(move || serve_sign(ours, signer, caller));
        let (stdin, stdout) = (child.stdin.take().expect("stdin"), child.stdout.take().expect("stdout"));
        self.running = Some(Running { child, wire: WireSource::new(BufReader::new(stdout), stdin) });
        self.starts += 1;
        Ok(())
    }

    fn call<T>(&mut self, f: impl FnOnce(&mut dyn JobSource) -> Result<T, SourceError>) -> Result<T, SourceError> {
        if self.revoked {
            return Err(SourceError::new(SourceCode::SourceRevoked, "the source is revoked"));
        }
        if self.running.is_none() {
            self.start()?;
        }
        let got = f(&mut self.running.as_mut().expect("running").wire);
        match &got {
            Err(e) if e.code == SourceCode::SourceRevoked => {
                self.revoked = true;
                self.running = None;
            }
            // The wire broke: the sidecar ended. Start it again later.
            Err(e) if e.code == SourceCode::Unavailable && e.detail.starts_with("the ") => {
                self.running = None;
                self.not_before = Some(Instant::now() + self.backoff);
                self.backoff = (self.backoff * 2).min(BACKOFF_MAX);
            }
            Ok(_) => self.backoff = BACKOFF_FIRST,
            _ => {}
        }
        got
    }
}

impl JobSource for SidecarSource {
    fn hello(&mut self, client: &str, version: &str) -> Result<Hello, SourceError> {
        self.call(|s| s.hello(client, version))
    }
    fn next(&mut self, capacity: u32, wait: Duration) -> Result<Vec<Event>, SourceError> {
        self.call(|s| s.next(capacity, wait))
    }
    fn ack(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.call(|s| s.ack(job))
    }
    fn claim(&mut self, job: &JobId, runner: &str) -> Result<(), SourceError> {
        self.call(|s| s.claim(job, runner))
    }
    fn mint(&mut self, job: &JobId, runner: &str) -> Result<Credential, SourceError> {
        self.call(|s| s.mint(job, runner))
    }
    fn report(&mut self, job: &JobId, report: &Map<String, Value>) -> Result<(), SourceError> {
        self.call(|s| s.report(job, report))
    }
    fn audit(&mut self, line: &Map<String, Value>) -> Result<(), SourceError> {
        self.call(|s| s.audit(line))
    }
    fn withdraw(&mut self, job: &JobId, reason: &str) -> Result<(), SourceError> {
        self.call(|s| s.withdraw(job, reason))
    }
    fn release(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.call(|s| s.release(job))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use podrunner_clock::SystemClock;

    use super::*;

    fn signer() -> Arc<Mutex<Signer>> {
        let out = Command::new("openssl").args(["genrsa", "-traditional", "2048"]).output().unwrap();
        let key = crate::sign::key_from_pem(&String::from_utf8(out.stdout).unwrap()).unwrap();
        Arc::new(Mutex::new(Signer::new(key, "1", Arc::new(SystemClock::default()), Box::new(|| 0))))
    }

    /// A sidecar that ends at once: each call is unavailable, the restart
    /// waits for the backoff, and the backoff grows.
    #[test]
    fn a_sidecar_that_ends_starts_again_after_a_backoff() {
        let mut s = SidecarSource::new("true", vec![], signer(), "test");
        assert_eq!(s.hello("p", "0").unwrap_err().code, SourceCode::Unavailable);
        assert_eq!(s.starts, 1);
        // Within the backoff, no start.
        assert!(s.hello("p", "0").unwrap_err().detail.contains("waits for its restart"));
        assert_eq!(s.starts, 1);
        assert_eq!(s.backoff, BACKOFF_FIRST * 2);
    }

    #[test]
    fn a_missing_program_is_unavailable() {
        let mut s = SidecarSource::new("/nonexistent/sidecar", vec![], signer(), "test");
        assert_eq!(s.hello("p", "0").unwrap_err().code, SourceCode::Unavailable);
    }
}
