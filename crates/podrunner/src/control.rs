//! The control socket (GitHub #22): JSON lines on a unix socket of mode
//! 0600. A peer with another uid is refused. Each request is one line with
//! `"v": 1` first; the reply is one line. The module sits behind the trait
//! [`Handler`], so that `pod-control` of podcrates (PodUtils/podcrates
//! T-103) can replace it with no change of the wire.
//!
//! Where units run as the uid of podrunner (a shared principal), the peer
//! check protects nothing against a unit: `docs/security/` says so.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The longest request line.
pub const LINE_MAX: usize = 64 * 1024;

/// A request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Request {
    Version,
    Status,
    Drain,
    Stop,
    Capacity,
    /// The operator looked at the host: start jobs again.
    ClearDegraded,
    Withdraw {
        job_id: String,
    },
}

#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    v: u32,
    #[serde(flatten)]
    body: T,
}

/// A reply: `ok` with a value, or an error with a code and a detail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Reply {
    Ok { value: Value },
    Error { code: String, detail: String },
}

impl Reply {
    pub fn error(code: &str, detail: impl Into<String>) -> Reply {
        Reply::Error { code: code.into(), detail: detail.into() }
    }
}

/// What answers a request: the supervisor.
pub trait Handler {
    fn handle(&mut self, req: &Request) -> Reply;
}

/// Read one request line. The detail of an error holds no byte of it.
pub fn parse(line: &[u8]) -> Result<Request, Reply> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    if line.len() > LINE_MAX {
        return Err(Reply::error("bad-request", format!("a line over {LINE_MAX} bytes")));
    }
    let v: Value = serde_json::from_slice(line).map_err(|_| Reply::error("bad-request", "not JSON"))?;
    if v.get("v") != Some(&Value::from(1)) {
        return Err(Reply::error("bad-request", "no \"v\": 1"));
    }
    serde_json::from_value::<Envelope<Request>>(v)
        .map(|e| e.body)
        .map_err(|_| Reply::error("bad-request", "an unknown op or field"))
}

fn line_of<T: Serialize>(body: &T) -> String {
    let mut s = serde_json::to_string(&Envelope { v: 1, body }).expect("JSON");
    s.push('\n');
    s
}

/// Bind the socket at `path` with mode 0600 (GitHub #54). A stale socket
/// (no one answers) goes first; a live socket is `AddrInUse`, and a path
/// that is not a socket is `AlreadyExists`: neither is removed.
pub fn bind(path: &Path) -> std::io::Result<UnixListener> {
    use std::os::unix::fs::FileTypeExt;
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
        Ok(m) if m.file_type().is_socket() => {
            if UnixStream::connect(path).is_ok() {
                return Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "a live socket"));
            }
            std::fs::remove_file(path)?;
        }
        Ok(_) => return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "not a socket")),
    }
    let l = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(l)
}

/// The uid of the peer of a unix stream.
pub fn peer_uid(s: &UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: getsockopt writes at most `len` bytes into `cred`, a ucred.
    let rc = unsafe {
        libc::getsockopt(
            s.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    (rc == 0).then_some(cred.uid)
}

fn my_uid() -> u32 {
    // SAFETY: getuid(2) cannot fail.
    unsafe { libc::getuid() }
}

/// The clients served at one time; one more gets `busy`.
pub const CLIENTS: usize = 8;

/// The absolute deadline of one exchange: the request line, and the reply
/// of the supervisor.
pub const EXCHANGE: Duration = Duration::from_secs(5);

/// A request for the supervisor, and where its reply goes.
pub struct Call {
    pub req: Request,
    pub reply: std::sync::mpsc::Sender<Reply>,
}

/// Serve `l` in threads of its own (GitHub #56): the supervisor gets whole
/// requests from the channel, so a slow or silent peer delays no tick.
pub fn spawn(l: UnixListener) -> std::sync::mpsc::Receiver<Call> {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (tx, rx) = std::sync::mpsc::channel();
    let active = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for stream in l.incoming() {
            let Ok(stream) = stream else { continue };
            if active.load(Ordering::SeqCst) >= CLIENTS {
                let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                let _ = (&stream).write_all(line_of(&Reply::error("busy", "too many clients")).as_bytes());
                continue;
            }
            active.fetch_add(1, Ordering::SeqCst);
            let (tx, active) = (tx.clone(), active.clone());
            std::thread::spawn(move || {
                let _ = exchange(stream, &tx);
                active.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    rx
}

/// Read one line before `deadline`, at most `LINE_MAX` bytes and a newline.
fn read_line(mut s: &UnixStream, deadline: std::time::Instant) -> std::io::Result<Vec<u8>> {
    let mut line = Vec::new();
    let mut buf = [0u8; 4096];
    while !line.contains(&b'\n') && line.len() <= LINE_MAX + 1 {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        s.set_read_timeout(Some(left))?;
        match s.read(&mut buf)? {
            0 => break,
            n => line.extend_from_slice(&buf[..n]),
        }
    }
    Ok(line)
}

fn exchange(stream: UnixStream, tx: &std::sync::mpsc::Sender<Call>) -> std::io::Result<()> {
    let deadline = std::time::Instant::now() + EXCHANGE;
    stream.set_write_timeout(Some(EXCHANGE))?;
    let mut writer = &stream;
    if peer_uid(&stream) != Some(my_uid()) {
        return writer.write_all(line_of(&Reply::error("refused", "the peer has another uid")).as_bytes());
    }
    let line = read_line(&stream, deadline)?;
    let reply = match parse(&line) {
        Err(r) => r,
        Ok(req) => {
            let (rtx, rrx) = std::sync::mpsc::channel();
            if tx.send(Call { req, reply: rtx }).is_err() {
                return Ok(());
            }
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            rrx.recv_timeout(left.max(Duration::from_secs(1)))
                .unwrap_or_else(|_| Reply::error("busy", "the supervisor did not answer in time"))
        }
    };
    writer.write_all(line_of(&reply).as_bytes())
}

/// Answer each call that waits now, with `h`.
pub fn answer_waiting(rx: &std::sync::mpsc::Receiver<Call>, h: &mut dyn Handler, wait: Duration) {
    let Ok(first) = rx.recv_timeout(wait) else { return };
    let mut next = Some(first);
    while let Some(call) = next {
        let _ = call.reply.send(h.handle(&call.req));
        next = rx.try_recv().ok();
    }
}

/// Send one request to the socket at `path`.
pub fn call(path: &Path, req: &Request) -> std::io::Result<Reply> {
    exchange_on(UnixStream::connect(path)?, req)
}

/// One request and its reply on `s`. The server can answer `busy` and
/// close before the request is written: the write then fails, but the
/// reply is still in the socket, so read it before the write error counts.
fn exchange_on(mut s: UnixStream, req: &Request) -> std::io::Result<Reply> {
    s.set_read_timeout(Some(Duration::from_secs(30)))?;
    let written = s.write_all(line_of(req).as_bytes());
    let mut line = Vec::new();
    let read = BufReader::new(s).take(16 * 1024 * 1024).read_until(b'\n', &mut line);
    if line.is_empty() {
        written?;
        read?;
    }
    let v: Envelope<Reply> = serde_json::from_slice(&line).map_err(std::io::Error::other)?;
    Ok(v.body)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo(Vec<Request>);

    impl Handler for Echo {
        fn handle(&mut self, req: &Request) -> Reply {
            self.0.push(req.clone());
            Reply::Ok { value: Value::from("done") }
        }
    }

    #[test]
    fn bad_lines_are_bad_requests() {
        for line in [&b"x"[..], br#"{"op":"status"}"#, br#"{"v":2,"op":"status"}"#, br#"{"v":1,"op":"rm"}"#] {
            assert!(matches!(parse(line), Err(Reply::Error { code, .. }) if code == "bad-request"));
        }
        assert_eq!(parse(br#"{"v":1,"op":"drain"}"#), Ok(Request::Drain));
        let long = vec![b' '; LINE_MAX + 1];
        assert!(parse(&long).is_err());
    }

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("podrunner-ctl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_socket_has_mode_0600_and_answers_one_request() {
        let dir = dir("one");
        let path = dir.join("ctl.sock");
        let rx = spawn(bind(&path).unwrap());
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let p = path.clone();
        let client = std::thread::spawn(move || call(&p, &Request::Status).unwrap());
        let mut h = Echo(Vec::new());
        while h.0.is_empty() {
            answer_waiting(&rx, &mut h, Duration::from_millis(50));
        }
        assert_eq!(client.join().unwrap(), Reply::Ok { value: Value::from("done") });
        assert_eq!(h.0, [Request::Status]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `busy` reply that comes, with a close, before the request is
    /// written is still the reply, not a broken pipe.
    #[test]
    fn a_reply_before_the_request_is_read() {
        let dir = dir("early");
        let path = dir.join("ctl.sock");
        let l = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            (&s).write_all(line_of(&Reply::error("busy", "too many clients")).as_bytes()).unwrap();
        });
        let s = UnixStream::connect(&path).unwrap();
        server.join().unwrap();
        // The server side is closed now: the write of the request fails.
        let r = exchange_on(s, &Request::Status).unwrap();
        assert!(matches!(&r, Reply::Error { code, .. } if code == "busy"), "{r:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn silent_clients_delay_no_answer_and_a_ninth_is_busy() {
        let dir = dir("idle");
        let path = dir.join("ctl.sock");
        let rx = spawn(bind(&path).unwrap());
        // Seven silent clients hold seven of the eight places.
        let idle: Vec<UnixStream> = (0..CLIENTS - 1).map(|_| UnixStream::connect(&path).unwrap()).collect();
        std::thread::sleep(Duration::from_millis(100));
        let p = path.clone();
        let started = std::time::Instant::now();
        let client = std::thread::spawn(move || call(&p, &Request::Status).unwrap());
        let mut h = Echo(Vec::new());
        while h.0.is_empty() && started.elapsed() < Duration::from_secs(3) {
            answer_waiting(&rx, &mut h, Duration::from_millis(50));
        }
        assert_eq!(client.join().unwrap(), Reply::Ok { value: Value::from("done") });
        assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
        // Eight silent clients now: a ninth gets busy.
        let _eighth = UnixStream::connect(&path).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert!(matches!(call(&path, &Request::Status).unwrap(), Reply::Error { code, .. } if code == "busy"));
        drop(idle);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_slow_client_is_cut_at_the_deadline() {
        let dir = dir("slow");
        let path = dir.join("ctl.sock");
        let _rx = spawn(bind(&path).unwrap());
        let mut s = UnixStream::connect(&path).unwrap();
        let started = std::time::Instant::now();
        // One byte each 500 ms, never a newline: the server ends the
        // exchange at its absolute deadline.
        let mut closed = false;
        while started.elapsed() < EXCHANGE + Duration::from_secs(3) {
            if s.write_all(b" ").is_err() {
                closed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        assert!(closed, "the exchange was not cut");
        assert!(started.elapsed() >= EXCHANGE, "{:?}", started.elapsed());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bind_keeps_a_live_socket_and_a_file_and_replaces_a_stale_socket() {
        let dir = dir("bind");
        let path = dir.join("ctl.sock");
        let live = bind(&path).unwrap();
        assert_eq!(bind(&path).unwrap_err().kind(), std::io::ErrorKind::AddrInUse);
        drop(live);
        // The file of a dead listener is stale: it is replaced.
        assert!(bind(&path).is_ok());
        let file = dir.join("file");
        std::fs::write(&file, "x").unwrap();
        assert_eq!(bind(&file).unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&file).unwrap(), b"x");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
