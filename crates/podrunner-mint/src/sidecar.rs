//! A source in a child process: the scale-set sidecar (GitHub #29). `mint`
//! starts it with the source wire on its stdio and a channel to `sign` on
//! its descriptor 3 (call C15), and starts it again with a backoff when it
//! ends, except after a revocation. Each call has a reply deadline
//! (GitHub #57): a sidecar that lives and says nothing is killed and starts
//! again after the backoff, so the supervisor never waits for ever, and no
//! late reply is read as the answer to the next request.

use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use podrunner_schema::{Credential, JobId};
use podrunner_source::wire::LINE_MAX;
use podrunner_source::{Event, Exchange, Hello, JobSource, SourceCode, SourceError, Wire};
use serde_json::{Map, Value};

use crate::sign::{Refusal, Signer};

/// The first and the longest wait before a restart.
const BACKOFF_FIRST: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(60);

/// The longest wait for one reply of the sidecar, after the long poll of
/// `next`. A call to GitHub inside the sidecar fits in it.
pub const REPLY_LIMIT: Duration = Duration::from_secs(15);

/// The stdio of the sidecar, with a deadline on each reply.
struct Pipe {
    reader: BufReader<ChildStdout>,
    writer: ChildStdin,
    /// The deadline of the next exchange.
    limit: Duration,
}

impl Exchange for Pipe {
    fn exchange(&mut self, line: &str) -> Result<Vec<u8>, SourceError> {
        let down = |e: std::io::Error| SourceError::new(SourceCode::Unavailable, format!("the wire: {}", e.kind()));
        self.writer.write_all(line.as_bytes()).map_err(down)?;
        self.writer.flush().map_err(down)?;
        let deadline = Instant::now() + self.limit;
        let mut reply = Vec::new();
        loop {
            let buf = self.reader.buffer();
            if let Some(i) = buf.iter().position(|b| *b == b'\n') {
                reply.extend_from_slice(&buf[..=i]);
                self.reader.consume(i + 1);
                return Ok(reply);
            }
            let n = buf.len();
            reply.extend_from_slice(buf);
            self.reader.consume(n);
            // One byte over the limit tells a long line from a full one.
            if reply.len() > LINE_MAX + 1 {
                return Ok(reply);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            let mut fd = libc::pollfd { fd: self.reader.get_ref().as_raw_fd(), events: libc::POLLIN, revents: 0 };
            let ms = left.as_millis().min(i32::MAX as u128) as i32;
            // SAFETY: poll(2) on one pollfd that lives on this stack.
            let rc = unsafe { libc::poll(&mut fd, 1, ms) };
            if rc < 0 {
                return Err(down(std::io::Error::last_os_error()));
            }
            if rc == 0 {
                return Err(SourceError::new(SourceCode::Unavailable, "the sidecar did not answer in time"));
            }
            if self.reader.fill_buf().map_err(down)?.is_empty() {
                return Err(SourceError::new(SourceCode::Unavailable, "the source closed the wire"));
            }
        }
    }
}

struct Running {
    child: Child,
    wire: Wire<Pipe>,
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
    /// The reply deadline of each call (`REPLY_LIMIT`).
    limit: Duration,
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
            limit: REPLY_LIMIT,
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
        let pipe = Pipe { reader: BufReader::new(stdout), writer: stdin, limit: REPLY_LIMIT };
        self.running = Some(Running { child, wire: Wire(pipe) });
        self.starts += 1;
        Ok(())
    }

    /// One call of the wire, with `wait` (the long poll of `next`) added to
    /// the reply deadline. A timeout is a broken wire: the child goes.
    fn call<T>(
        &mut self,
        wait: Duration,
        f: impl FnOnce(&mut dyn JobSource) -> Result<T, SourceError>,
    ) -> Result<T, SourceError> {
        if self.revoked {
            return Err(SourceError::new(SourceCode::SourceRevoked, "the source is revoked"));
        }
        if self.running.is_none() {
            self.start()?;
        }
        let running = self.running.as_mut().expect("running");
        running.wire.0.limit = self.limit + wait;
        let got = f(&mut running.wire);
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
        self.call(Duration::ZERO, |s| s.hello(client, version))
    }
    fn next(&mut self, capacity: u32, wait: Duration) -> Result<Vec<Event>, SourceError> {
        self.call(wait, |s| s.next(capacity, wait))
    }
    fn ack(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.call(Duration::ZERO, |s| s.ack(job))
    }
    fn claim(&mut self, job: &JobId, runner: &str) -> Result<(), SourceError> {
        self.call(Duration::ZERO, |s| s.claim(job, runner))
    }
    fn mint(&mut self, job: &JobId, runner: &str) -> Result<Credential, SourceError> {
        self.call(Duration::ZERO, |s| s.mint(job, runner))
    }
    fn report(&mut self, job: &JobId, report: &Map<String, Value>) -> Result<(), SourceError> {
        self.call(Duration::ZERO, |s| s.report(job, report))
    }
    fn audit(&mut self, line: &Map<String, Value>) -> Result<(), SourceError> {
        self.call(Duration::ZERO, |s| s.audit(line))
    }
    fn withdraw(&mut self, job: &JobId, reason: &str) -> Result<(), SourceError> {
        self.call(Duration::ZERO, |s| s.withdraw(job, reason))
    }
    fn release(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.call(Duration::ZERO, |s| s.release(job))
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

    /// A script sidecar: answers `hello`, then `sh` runs `then`.
    fn script(then: &str) -> SidecarSource {
        let hello = r#"{"schema":1,"reply":"hello","source":"s","operator_controlled":true}"#;
        let body = format!("read l; echo '{hello}'; read l; {then}");
        SidecarSource::new("sh", vec!["-c".into(), body], signer(), "test")
    }

    #[test]
    fn a_silent_sidecar_is_killed_at_the_deadline_and_starts_again() {
        let mut s = script("sleep 30");
        s.limit = Duration::from_millis(500);
        s.hello("p", "0").unwrap();
        let t = Instant::now();
        let e = s.ack(&JobId::new("j-1").unwrap()).unwrap_err();
        assert_eq!(e.code, SourceCode::Unavailable);
        assert!(e.detail.contains("did not answer in time"), "{}", e.detail);
        assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
        assert!(s.running.is_none());
        // After the backoff, the next call starts a new sidecar.
        s.not_before = None;
        s.hello("p", "0").unwrap();
        assert_eq!(s.starts, 2);
    }

    #[test]
    fn a_late_reply_is_never_the_answer_to_the_next_call() {
        let late = r#"{"schema":1,"reply":"done"}"#;
        let mut s = script(&format!("sleep 1; echo '{late}'; sleep 30"));
        s.limit = Duration::from_millis(300);
        s.hello("p", "0").unwrap();
        assert!(s.ack(&JobId::new("j-1").unwrap()).is_err());
        std::thread::sleep(Duration::from_millis(1200));
        // The killed child cannot answer; a new child starts with hello.
        s.not_before = None;
        assert!(s.ack(&JobId::new("j-2").unwrap()).is_err());
    }

    #[test]
    fn a_child_that_ends_in_a_call_is_unavailable() {
        let mut s = script("exit 0");
        s.hello("p", "0").unwrap();
        let e = s.ack(&JobId::new("j-1").unwrap()).unwrap_err();
        assert_eq!(e.code, SourceCode::Unavailable, "{e:?}");
    }

    #[test]
    fn a_missing_program_is_unavailable() {
        let mut s = SidecarSource::new("/nonexistent/sidecar", vec![], signer(), "test");
        assert_eq!(s.hello("p", "0").unwrap_err().code, SourceCode::Unavailable);
    }
}
