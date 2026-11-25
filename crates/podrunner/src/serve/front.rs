//! The front of the egress socket of a unit of `serve` (T-493). The unit
//! sees one socket, where its proxy variables point. A request for
//! [`CONTROL`] is a call of the job-started hook of the runner image: a
//! question (`POST /v1/assigned`), which `serve` answers, or the mark of
//! the first step (`POST /v1/started`, T-660); each other request goes
//! unchanged to the egress module on its own socket, which holds the
//! policy of the job. The front reads a bounded head within a time limit,
//! and serves a bounded count of connections (the lessons of T-412).

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

/// The name of `serve` in a unit: not a loopback name, so a client sends
/// its request through its proxy, which is this front.
pub const CONTROL: &str = "control.podrunner.test";

/// The longest head of a request.
const HEAD_MAX: usize = 64 * 1024;

/// The longest body of a question.
const BODY_MAX: usize = 64 * 1024;

/// The routes of the control host.
pub const ROUTES: [&str; 2] = ["/v1/assigned", "/v1/started"];

/// The answer of `serve` to a call: the route (one of [`ROUTES`]) and the
/// body give a JSON text.
pub type Ask = Arc<dyn Fn(&str, &[u8]) -> String + Send + Sync>;

/// The bounds of a front.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    /// The time to read a head (and the body of a question).
    pub head: Duration,
    /// The most connections at one time.
    pub conns: usize,
}

impl Default for Bounds {
    fn default() -> Bounds {
        Bounds { head: Duration::from_secs(30), conns: 128 }
    }
}

/// A front that serves one socket until it is dropped.
pub struct Front {
    path: PathBuf,
    stop: Arc<AtomicBool>,
}

impl Drop for Front {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_file(&self.path);
    }
}

/// The host of a request target: `host:port` of a CONNECT, or the host of
/// `http://host[:port]/path`.
fn host(method: &str, target: &str) -> String {
    let authority = if method == "CONNECT" {
        target
    } else {
        let rest = target.strip_prefix("http://").or_else(|| target.strip_prefix("https://")).unwrap_or("");
        rest.split('/').next().unwrap_or("")
    };
    let h =
        authority.rsplit_once(':').filter(|(_, p)| p.bytes().all(|b| b.is_ascii_digit())).map_or(authority, |x| x.0);
    h.trim_end_matches('.').to_ascii_lowercase()
}

fn answer(conn: &UnixStream, status: &str, body: &str) -> std::io::Result<()> {
    let msg = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    (&*conn).write_all(msg.as_bytes())
}

/// The head of a request: its first line, its other lines (as bytes: a
/// header need not be UTF-8), and the bytes read after it.
struct Head {
    first: String,
    raw_first: Vec<u8>,
    lines: Vec<u8>,
    rest: Vec<u8>,
}

/// The head, or `None` when it passed `HEAD_MAX` or the peer ended.
fn read_head(conn: &UnixStream) -> std::io::Result<Option<Head>> {
    let mut r = BufReader::new(conn.take(HEAD_MAX as u64));
    let mut raw_first = Vec::new();
    if r.read_until(b'\n', &mut raw_first)? == 0 || !raw_first.ends_with(b"\n") {
        return Ok(None);
    }
    let mut lines = Vec::new();
    loop {
        let mut l = Vec::new();
        if r.read_until(b'\n', &mut l)? == 0 || !l.ends_with(b"\n") {
            return Ok(None);
        }
        lines.extend_from_slice(&l);
        if l == b"\r\n" || l == b"\n" {
            break;
        }
    }
    let rest = r.buffer().to_vec();
    let first = String::from_utf8_lossy(&raw_first).into_owned();
    Ok(Some(Head { first, raw_first, lines, rest }))
}

/// A call to `serve`: `POST` of a route of [`ROUTES`] with a JSON body.
fn question(conn: &UnixStream, method: &str, target: &str, head: &Head, ask: &Ask) -> std::io::Result<()> {
    let path = target.split_once("://").and_then(|(_, t)| t.find('/').map(|i| &t[i..])).unwrap_or("/");
    if method != "POST" || !ROUTES.contains(&path) {
        return answer(conn, "404 Not Found", r#"{"error":"POST /v1/assigned or /v1/started only"}"#);
    }
    let length = String::from_utf8_lossy(&head.lines)
        .lines()
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<usize>().ok());
    let Some(length) = length.filter(|n| *n <= BODY_MAX) else {
        return answer(conn, "411 Length Required", r#"{"error":"a Content-Length of 65536 or less"}"#);
    };
    let mut body = head.rest.clone();
    body.truncate(length);
    if body.len() < length {
        let mut more = vec![0; length - body.len()];
        (&*conn).read_exact(&mut more)?;
        body.extend_from_slice(&more);
    }
    answer(conn, "200 OK", &ask(path, &body))
}

/// Join the connection of the unit and the egress module, both ways, and
/// return when both ways ended: the bound of the connections counts each
/// thread and each descriptor of a connection.
fn join(conn: UnixStream, mut up: UnixStream) -> std::io::Result<()> {
    let mut back = conn.try_clone()?;
    let mut up_read = up.try_clone()?;
    let c1 = conn.try_clone()?;
    let back_way = std::thread::Builder::new().spawn(move || {
        let _ = std::io::copy(&mut up_read, &mut back);
        let _ = c1.shutdown(std::net::Shutdown::Both);
    })?;
    let mut conn = conn;
    let _ = std::io::copy(&mut conn, &mut up);
    let _ = up.shutdown(std::net::Shutdown::Write);
    let _ = back_way.join();
    Ok(())
}

/// One place of the bound of connections, given back at its drop (also at
/// a panic of its thread).
struct Place(Arc<AtomicUsize>);

impl Drop for Place {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn one(conn: UnixStream, inner: &Path, ask: &Ask, head_time: Duration) -> std::io::Result<()> {
    conn.set_read_timeout(Some(head_time))?;
    let Some(head) = read_head(&conn)? else {
        return answer(&conn, "431 Request Header Fields Too Large", r#"{"error":"no head within 65536 bytes"}"#);
    };
    let mut words = head.first.split_whitespace();
    let (method, target) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
    if host(method, target) == CONTROL {
        return question(&conn, method, target, &head, ask);
    }
    // A tunnel or a request can be idle for long: no time limit after the
    // head.
    conn.set_read_timeout(None)?;
    let mut up = UnixStream::connect(inner)?;
    up.write_all(&[head.raw_first.as_slice(), &head.lines, &head.rest].concat())?;
    join(conn, up)
}

/// Serve the socket `path` of a unit (mode 0600): `inner` is the socket of
/// the egress module of the unit, and `ask` answers a question.
pub fn serve(path: &Path, inner: PathBuf, ask: Ask, bounds: Bounds) -> std::io::Result<Front> {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    let live = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        while !s.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((conn, _)) => {
                    let _ = conn.set_nonblocking(false);
                    let place = Place(live.clone());
                    if live.fetch_add(1, Ordering::SeqCst) >= bounds.conns {
                        let _ = answer(&conn, "503 Service Unavailable", r#"{"error":"too many connections"}"#);
                        continue;
                    }
                    let (inner, ask) = (inner.clone(), ask.clone());
                    // No thread for it (a limit of the host): the connection
                    // closes, and the place goes back with it.
                    let _ = std::thread::Builder::new().spawn(move || {
                        let _place = place;
                        let _ = one(conn, &inner, &ask, bounds.head);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(20)),
                // A failed accept (no descriptor left in the process) does
                // not end the front: try again after a pause.
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    });
    Ok(Front { path: path.to_path_buf(), stop })
}

#[cfg(test)]
#[path = "front_tests.rs"]
mod tests;
