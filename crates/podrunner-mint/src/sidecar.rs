//! A source in a child process: the scale-set sidecar (GitHub #29), or any
//! program that speaks the source wire (the `program` kind, T-101). `mint`
//! starts it with the source wire on its stdio and, for the sidecar, a
//! channel to `sign` on its descriptor 3 (call C15), and starts it again
//! with a backoff when it
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
    /// The wire itself failed: a write, a read, a deadline or an end. A
    /// reply of the source, also an error, is no failure of the wire (#234).
    broken: bool,
}

impl Exchange for Pipe {
    fn exchange(&mut self, line: &str) -> Result<Vec<u8>, SourceError> {
        let r = self.exchange_once(line);
        self.broken |= r.is_err();
        r
    }
}

impl Pipe {
    fn exchange_once(&mut self, line: &str) -> Result<Vec<u8>, SourceError> {
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
    /// The environment that the program gets beside the one of podrunner.
    env: Vec<(String, String)>,
    /// `sign` on descriptor 3: the sidecar only.
    signer: Option<Arc<Mutex<Signer>>>,
    /// Or a fine-grained token of the organization on descriptor 3, in place
    /// of `sign` (T-174): the sidecar asks for it once (`{"op":"pat"}`).
    token: Option<Arc<String>>,
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

/// What descriptor 3 of a sidecar answers.
enum Answer {
    Sign(Arc<Mutex<Signer>>),
    Token(Arc<String>),
}

/// Answer each request of one sidecar with the token until its channel
/// closes (T-174).
fn serve_token(stream: UnixStream, token: &str) {
    let Ok(mut w) = stream.try_clone() else { return };
    for line in BufReader::new(stream).lines() {
        if line.is_err() {
            return;
        }
        let reply = serde_json::json!({ "token": token });
        if w.write_all(format!("{reply}\n").as_bytes()).is_err() {
            return;
        }
    }
}

/// Answer the sign requests of one sidecar until its channel closes.
fn serve_sign(stream: UnixStream, signer: Arc<Mutex<Signer>>, caller: String) {
    let Ok(mut w) = stream.try_clone() else { return };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        let req: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        let claims = req.get("claims").and_then(Value::as_object).cloned().unwrap_or_default();
        let signed = signer.lock().map(|mut s| s.sign(&caller, &claims).map(|t| (t, s.last_exp())));
        let reply = match signed {
            // `exp` of the token: a token given again has an older one.
            Ok(Ok((token, exp))) => serde_json::json!({ "token": token, "exp": exp }),
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
            env: Vec::new(),
            signer: Some(signer),
            token: None,
            caller: caller.into(),
            running: None,
            backoff: BACKOFF_FIRST,
            not_before: None,
            revoked: false,
            starts: 0,
            limit: REPLY_LIMIT,
        }
    }

    /// A program that speaks the source wire on its stdio, with no `sign`
    /// channel (T-101). `env` gives it, for example, the path of its
    /// credential file: never the credential itself.
    pub fn program(program: &str, args: Vec<String>, env: Vec<(String, String)>, caller: &str) -> SidecarSource {
        SidecarSource {
            program: program.into(),
            args,
            env,
            signer: None,
            token: None,
            caller: caller.into(),
            running: None,
            backoff: BACKOFF_FIRST,
            not_before: None,
            revoked: false,
            starts: 0,
            limit: REPLY_LIMIT,
        }
    }

    /// The sidecar with a fine-grained token of the organization on its
    /// descriptor 3, in place of `sign` (T-174). The token never goes in
    /// argv or in the environment.
    pub fn with_token(program: &str, args: Vec<String>, token: String, caller: &str) -> SidecarSource {
        let mut s = SidecarSource::program(program, args, Vec::new(), caller);
        s.token = Some(Arc::new(token));
        s
    }

    fn start(&mut self) -> Result<(), SourceError> {
        if self.not_before.is_some_and(|t| Instant::now() < t) {
            return Err(SourceError::new(SourceCode::Unavailable, "the sidecar waits for its restart"));
        }
        let down =
            |what: &str, e: std::io::Error| SourceError::new(SourceCode::Unavailable, format!("{what}: {}", e.kind()));
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args).envs(self.env.iter().map(|(k, v)| (k, v)));
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
        let answer: Option<Answer> = match (&self.signer, &self.token) {
            (Some(signer), _) => Some(Answer::Sign(signer.clone())),
            (None, Some(token)) => Some(Answer::Token(token.clone())),
            _ => None,
        };
        let channel = match answer {
            Some(answer) => {
                let (ours, theirs) = UnixStream::pair().map_err(|e| down("the sign channel", e))?;
                let fd = theirs.as_raw_fd();
                // SAFETY: dup2 in the child before exec, with a descriptor
                // that is open in the parent until the spawn returns.
                unsafe {
                    cmd.pre_exec(
                        move || {
                            if libc::dup2(fd, 3) < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
                        },
                    );
                }
                Some((ours, theirs, answer))
            }
            None => None,
        };
        let mut child = cmd.spawn().map_err(|e| down("the sidecar", e))?;
        if let Some((ours, theirs, answer)) = channel {
            drop(theirs);
            let caller = self.caller.clone();
            std::thread::spawn(move || match answer {
                Answer::Sign(signer) => serve_sign(ours, signer, caller),
                Answer::Token(token) => serve_token(ours, &token),
            });
        }
        let (stdin, stdout) = (child.stdin.take().expect("stdin"), child.stdout.take().expect("stdout"));
        let pipe = Pipe { reader: BufReader::new(stdout), writer: stdin, limit: REPLY_LIMIT, broken: false };
        self.running = Some(Running { child, wire: Wire(pipe) });
        self.starts += 1;
        Ok(())
    }

    /// One call of the wire, with `wait` (the long poll of `next`) added to
    /// the reply deadline. A timeout is a broken wire: the child goes. Only
    /// a call that `resets` (a `next` that passed) resets the backoff: a
    /// sidecar that answers `hello` and then ends must not restart each
    /// second (T-165).
    fn call<T>(
        &mut self,
        wait: Duration,
        resets: bool,
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
        running.wire.0.broken = false;
        let got = f(&mut running.wire);
        let broken = running.wire.0.broken;
        match &got {
            Err(e) if e.code == SourceCode::SourceRevoked => {
                self.revoked = true;
                self.running = None;
            }
            // The wire broke: the sidecar ended. Start it again later. An
            // error that the source sent goes to the caller as it is.
            Err(_) if broken => {
                self.running = None;
                self.not_before = Some(Instant::now() + self.backoff);
                self.backoff = (self.backoff * 2).min(BACKOFF_MAX);
            }
            Ok(_) if resets => self.backoff = BACKOFF_FIRST,
            _ => {}
        }
        got
    }
}

impl JobSource for SidecarSource {
    fn hello(&mut self, client: &str, version: &str) -> Result<Hello, SourceError> {
        self.call(Duration::ZERO, false, |s| s.hello(client, version))
    }
    fn next(&mut self, capacity: u32, wait: Duration) -> Result<Vec<Event>, SourceError> {
        self.call(wait, true, |s| s.next(capacity, wait))
    }
    fn ack(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.call(Duration::ZERO, false, |s| s.ack(job))
    }
    fn claim(&mut self, job: &JobId, runner: &str) -> Result<(), SourceError> {
        self.call(Duration::ZERO, false, |s| s.claim(job, runner))
    }
    fn mint(&mut self, job: &JobId, runner: &str) -> Result<Credential, SourceError> {
        self.call(Duration::ZERO, false, |s| s.mint(job, runner))
    }
    fn report(&mut self, job: &JobId, report: &Map<String, Value>) -> Result<(), SourceError> {
        self.call(Duration::ZERO, false, |s| s.report(job, report))
    }
    fn audit(&mut self, line: &Map<String, Value>) -> Result<(), SourceError> {
        self.call(Duration::ZERO, false, |s| s.audit(line))
    }
    fn withdraw(&mut self, job: &JobId, reason: &str) -> Result<(), SourceError> {
        self.call(Duration::ZERO, false, |s| s.withdraw(job, reason))
    }
    fn release(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.call(Duration::ZERO, false, |s| s.release(job))
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

    /// #234: an error that the source sends keeps its sidecar, also when its
    /// text starts with "the ".
    #[test]
    fn an_error_of_the_source_keeps_its_sidecar() {
        let err = r#"{"schema":1,"reply":"error","code":"unavailable","detail":"the API is down"}"#;
        let mut s = script(&format!("echo '{err}'; read l; echo '{err}'; sleep 5"));
        s.hello("p", "0").unwrap();
        let e = s.next(1, Duration::ZERO).unwrap_err();
        assert_eq!(e.detail, "the API is down");
        let e = s.next(1, Duration::ZERO).unwrap_err();
        assert_eq!(e.detail, "the API is down", "the sidecar stays: its next answer comes");
        assert_eq!(s.starts, 1);
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

    /// T-165: a sidecar that answers `hello` and ends at `next` gets a
    /// backoff that grows; `hello` does not reset it.
    #[test]
    fn a_sidecar_that_ends_after_each_hello_backs_off_more() {
        let mut s = script("exit 0");
        for want in [2, 4, 8] {
            s.not_before = None;
            s.hello("p", "0").unwrap();
            assert_eq!(s.next(1, Duration::ZERO).unwrap_err().code, SourceCode::Unavailable);
            assert_eq!(s.backoff, BACKOFF_FIRST * want);
        }
        // A `next` that passes resets it.
        let mut ok = script(r#"echo '{"schema":1,"reply":"events","events":[]}'; sleep 5"#);
        ok.backoff = BACKOFF_FIRST * 8;
        ok.hello("p", "0").unwrap();
        assert_eq!(ok.backoff, BACKOFF_FIRST * 8);
        ok.next(1, Duration::ZERO).unwrap();
        assert_eq!(ok.backoff, BACKOFF_FIRST);
    }

    /// T-174: a sidecar with a token gets it on its descriptor 3, and the
    /// token is in no argument of the sidecar.
    #[test]
    fn a_token_comes_on_descriptor_3_and_not_in_argv() {
        let hello = r#"{"schema":1,"reply":"hello","source":"s","operator_controlled":true}"#;
        let got = std::env::temp_dir().join(format!("podrunner-pat-{}", std::process::id()));
        let body = format!(
            "echo '{{\"op\":\"pat\"}}' >&3; read t <&3; printf '%s' \"$t\" > '{}'; read l; echo '{hello}'; sleep 5",
            got.display()
        );
        let mut s = SidecarSource::with_token("sh", vec!["-c".into(), body.clone()], "made-up-pat-55".into(), "test");
        assert!(!body.contains("made-up-pat-55"));
        s.hello("p", "0").unwrap();
        let text = std::fs::read_to_string(&got).unwrap();
        let _ = std::fs::remove_file(&got);
        assert_eq!(text, r#"{"token":"made-up-pat-55"}"#);
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
