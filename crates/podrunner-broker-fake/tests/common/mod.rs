//! The helpers of the tests that run the official runner.
// Each test file uses a part of the helpers.
#![allow(unused_imports, dead_code)]

pub use podrunner_broker_fake::runner::{rsaparams, run_once};

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Copy each byte from `a` to `b` until `a` ends, then end the write side
/// of `b`.
fn pipe(mut a: impl Read + Send + 'static, mut b: impl Write + Send + 'static, end: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut a, &mut b);
        end();
    });
}

/// One connection of the proxy: a CONNECT becomes a tunnel to the broker;
/// a request in absolute form goes to the broker in origin form.
fn proxy_one(conn: UnixStream, broker: u16, log: &Mutex<Vec<String>>) -> std::io::Result<()> {
    log.lock().unwrap().push("accept".into());
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
    }
    log.lock().unwrap().push(first.trim_end().to_string());
    let mut up = TcpStream::connect(("127.0.0.1", broker))?;
    let mut words = first.split_whitespace();
    let (method, target, version) =
        (words.next().unwrap_or(""), words.next().unwrap_or(""), words.next().unwrap_or(""));
    let mut back = conn.try_clone()?;
    if method == "CONNECT" {
        back.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")?;
    } else {
        let path = target.strip_prefix("http://").and_then(|t| t.find('/').map(|i| &t[i..])).unwrap_or(target);
        up.write_all(format!("{method} {path} {version}\r\n{head}").as_bytes())?;
    }
    up.write_all(r.buffer())?;
    let (u1, u2) = (up.try_clone()?, up);
    let c1 = conn.try_clone()?;
    pipe(u1, back, move || {
        let _ = c1.shutdown(std::net::Shutdown::Both);
    });
    pipe(conn, u2.try_clone()?, move || {
        let _ = u2.shutdown(std::net::Shutdown::Write);
    });
    Ok(())
}

/// The proxy on the egress socket `path`, and the log of its request lines.
pub fn proxy(path: &Path, broker: u16) -> Arc<Mutex<Vec<String>>> {
    let listener = UnixListener::bind(path).unwrap();
    let log = Arc::new(Mutex::new(Vec::new()));
    let l = log.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let l = l.clone();
            std::thread::spawn(move || {
                let _ = proxy_one(conn, broker, &l);
            });
        }
    });
    log
}

pub fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// T-493, T-530: a stand-in of the front of the egress socket of `serve`
/// that answers each question of the job-started hook with one verdict.
pub type Seen = Arc<Mutex<Vec<String>>>;

/// One question to the stand-in of the front: its head and body go to
/// `seen`, and the answer is `verdict`.
pub fn front_one<C>(c: C, verdict: &str, seen: &Seen)
where
    for<'a> &'a C: std::io::Read + std::io::Write,
{
    use std::io::{BufRead, BufReader, Read, Write};
    let mut r = BufReader::new(&c);
    let mut head = String::new();
    let mut len = 0;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
        head.push_str(&line);
    }
    let mut body = vec![0; len];
    let _ = r.read_exact(&mut body);
    seen.lock().unwrap().push(format!("{head}\n{}", String::from_utf8_lossy(&body)));
    let _ = (&c).write_all(
        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{verdict}", verdict.len())
            .as_bytes(),
    );
}

/// The stand-in of the front on a TCP port, as the proxy of a unit.
pub fn front_stand_in(verdict: &'static str) -> (u16, Seen) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let seen = Seen::default();
    let keep = seen.clone();
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            front_one(c, verdict, &keep);
        }
    });
    (port, seen)
}

/// The stand-in of the front on a unix socket, as the egress socket of a
/// podvm unit (T-530).
pub fn front_stand_in_unix(verdict: &'static str, path: &std::path::Path) -> Seen {
    let _ = std::fs::remove_file(path);
    let l = std::os::unix::net::UnixListener::bind(path).unwrap();
    let seen = Seen::default();
    let keep = seen.clone();
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            front_one(c, verdict, &keep);
        }
    });
    seen
}
