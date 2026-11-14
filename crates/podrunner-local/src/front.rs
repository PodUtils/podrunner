//! The front of the egress socket of a guest unit (T-123). A unit of the
//! microvm or the incus kind has no NIC: its one way out is the channel to
//! its egress socket, where its proxy variables point. The fake broker of
//! the job listens on the loopback of the host, so the front serves the
//! egress socket: a request for [`BROKER`] goes to the broker (a CONNECT
//! becomes a tunnel; a request in absolute form goes in origin form), and
//! each other request goes unchanged to the egress module on its own socket,
//! which holds the policy of `--allow`. With no egress module, each other
//! request gets 403.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// The name of the broker in a guest: not a loopback name, so the runner
/// sends each request through its proxy.
pub const BROKER: &str = "broker.podrunner.test";

/// The longest head of a request.
const HEAD_MAX: usize = 64 * 1024;

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
    h.to_ascii_lowercase()
}

/// Copy each byte from `a` to `b` until `a` ends, then end the write side
/// of `b`.
fn pipe(mut a: impl Read + Send + 'static, mut b: impl Write + Send + 'static, end: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut a, &mut b);
        end();
    });
}

/// Join a stream of the guest and an upstream, both ways.
fn join<U: Read + Write + Send + 'static>(conn: UnixStream, up: U, up2: U, shut: fn(&U)) -> std::io::Result<()> {
    let back = conn.try_clone()?;
    let c1 = conn.try_clone()?;
    pipe(up, back, move || {
        let _ = c1.shutdown(std::net::Shutdown::Both);
    });
    let mut up2 = up2;
    std::thread::spawn(move || {
        let mut conn = conn;
        let _ = std::io::copy(&mut conn, &mut up2);
        shut(&up2);
    });
    Ok(())
}

fn one(conn: UnixStream, broker: u16, inner: Option<&Path>) -> std::io::Result<()> {
    let mut r = BufReader::new(conn.try_clone()?);
    let mut first = String::new();
    r.read_line(&mut first)?;
    let mut head = String::new();
    loop {
        let mut l = String::new();
        if r.read_line(&mut l)? == 0 || l == "\r\n" {
            head.push_str(&l);
            break;
        }
        head.push_str(&l);
        if head.len() > HEAD_MAX {
            return (&conn).write_all(b"HTTP/1.1 431 Request Header Fields Too Large\r\nContent-Length: 0\r\n\r\n");
        }
    }
    let mut words = first.split_whitespace();
    let (method, target, version) =
        (words.next().unwrap_or(""), words.next().unwrap_or(""), words.next().unwrap_or(""));
    let rest = r.buffer().to_vec();
    if host(method, target) == BROKER {
        let mut up = TcpStream::connect(("127.0.0.1", broker))?;
        if method == "CONNECT" {
            (&conn).write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")?;
        } else {
            let path = target.split_once("://").and_then(|(_, t)| t.find('/').map(|i| &t[i..])).unwrap_or("/");
            up.write_all(format!("{method} {path} {version}\r\n{head}").as_bytes())?;
        }
        up.write_all(&rest)?;
        let up2 = up.try_clone()?;
        return join(conn, up, up2, |u| {
            let _ = u.shutdown(std::net::Shutdown::Write);
        });
    }
    let Some(inner) = inner else {
        let body = format!("podrunner: the local mode allows no host but the broker: {target}\n");
        let msg =
            format!("HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        return (&conn).write_all(msg.as_bytes());
    };
    let mut up = UnixStream::connect(inner)?;
    up.write_all(first.as_bytes())?;
    up.write_all(head.as_bytes())?;
    up.write_all(&rest)?;
    let up2 = up.try_clone()?;
    join(conn, up, up2, |u| {
        let _ = u.shutdown(std::net::Shutdown::Write);
    })
}

/// Serve the socket `path` for a unit: the broker listens on the loopback
/// port `broker`; `inner` is the socket of the egress module, if any. The
/// socket has mode 0666: the mapped root of an incus instance connects to
/// it (inside the 0700 directory of the unit).
pub fn serve(path: &Path, broker: u16, inner: Option<PathBuf>) -> std::io::Result<Front> {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o666))?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    std::thread::spawn(move || {
        while !s.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((conn, _)) => {
                    let _ = conn.set_nonblocking(false);
                    let inner = inner.clone();
                    std::thread::spawn(move || {
                        let _ = one(conn, broker, inner.as_deref());
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(20)),
                Err(_) => break,
            }
        }
    });
    Ok(Front { path: path.to_path_buf(), stop })
}

#[cfg(test)]
mod tests;
