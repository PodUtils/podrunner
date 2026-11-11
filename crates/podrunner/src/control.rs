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

/// Bind the socket at `path` with mode 0600. An old socket file goes first.
pub fn bind(path: &Path) -> std::io::Result<UnixListener> {
    let _ = std::fs::remove_file(path);
    let l = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    l.set_nonblocking(true)?;
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

/// Answer each connection that waits now, one request each.
pub fn serve_waiting(l: &UnixListener, h: &mut dyn Handler) {
    while let Ok((stream, _)) = l.accept() {
        let _ = answer(stream, h);
    }
}

fn answer(stream: UnixStream, h: &mut dyn Handler) -> std::io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut writer = stream.try_clone()?;
    if peer_uid(&stream) != Some(my_uid()) {
        return writer.write_all(line_of(&Reply::error("refused", "the peer has another uid")).as_bytes());
    }
    let mut line = Vec::new();
    BufReader::new(stream).take(LINE_MAX as u64 + 2).read_until(b'\n', &mut line)?;
    let reply = match parse(&line) {
        Ok(req) => h.handle(&req),
        Err(r) => r,
    };
    writer.write_all(line_of(&reply).as_bytes())
}

/// Send one request to the socket at `path`.
pub fn call(path: &Path, req: &Request) -> std::io::Result<Reply> {
    let mut s = UnixStream::connect(path)?;
    s.set_read_timeout(Some(Duration::from_secs(30)))?;
    s.write_all(line_of(req).as_bytes())?;
    let mut line = Vec::new();
    BufReader::new(s).take(16 * 1024 * 1024).read_until(b'\n', &mut line)?;
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

    #[test]
    fn the_socket_has_mode_0600_and_answers_one_request() {
        let dir = std::env::temp_dir().join(format!("podrunner-ctl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ctl.sock");
        let l = bind(&path).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let p = path.clone();
        let client = std::thread::spawn(move || call(&p, &Request::Status).unwrap());
        let mut h = Echo(Vec::new());
        while h.0.is_empty() {
            serve_waiting(&l, &mut h);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(client.join().unwrap(), Reply::Ok { value: Value::from("done") });
        assert_eq!(h.0, [Request::Status]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
