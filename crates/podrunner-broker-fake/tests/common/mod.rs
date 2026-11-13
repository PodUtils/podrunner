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
