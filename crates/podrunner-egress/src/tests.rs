//! Two units, two listeners: each connection is the job of its listener,
//! with no token; a token of another job is refused; a close or a kill
//! ends the listener. The far end is an echo, and the resolver gives a
//! public address: no test leaves the host.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use pod_broker::{Broker, Settings};
use pod_core::{BoxFuture, Connected, Dial, Failure, Found, Resolve, Source, Target};

use super::Egress;

const ONLY_A: &str = r#"{"schema":1,"mode":"allow","allow":[{"host":"a.example"}]}"#;
const ONLY_B: &str = r#"{"schema":1,"mode":"allow","allow":[{"host":"b.example"}]}"#;

struct Public;

impl Resolve for Public {
    fn label(&self) -> String {
        "public".into()
    }
    fn resolve_from<'a>(
        &'a self,
        _: &'a str,
        port: u16,
        _: tokio::time::Instant,
        _: Option<Source>,
    ) -> BoxFuture<'a, Result<Found, Failure>> {
        let addr = SocketAddr::new("192.0.2.10".parse().unwrap(), port);
        Box::pin(
            async move { Ok(Found { addrs: vec![addr], source: Source { rank: 0, name: "public" }, more: false }) },
        )
    }
}

struct Echo;

impl Dial for Echo {
    fn label(&self) -> String {
        "echo".into()
    }
    fn dial<'a>(&'a self, _: &'a Target, _: tokio::time::Instant) -> BoxFuture<'a, Result<Connected, Failure>> {
        Box::pin(async move {
            let (ours, theirs) = tokio::io::duplex(64 * 1024);
            tokio::spawn(async move {
                let (mut r, mut w) = tokio::io::split(theirs);
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
            Ok(Connected::new(Box::new(ours)))
        })
    }
}

fn egress() -> Egress {
    Egress::new(Broker::new(Settings::default(), Arc::new(Public), Arc::new(Echo))).unwrap()
}

fn dir() -> PathBuf {
    let d =
        std::env::temp_dir().join(format!("podrunner-egress-{}-{:?}", std::process::id(), std::thread::current().id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The status of the answer to `request` on the socket, and the stream.
fn ask(socket: &std::path::Path, request: &str) -> (u16, BufReader<UnixStream>) {
    let mut s = UnixStream::connect(socket).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.write_all(request.as_bytes()).unwrap();
    let mut r = BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line).unwrap();
    let status = line.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    loop {
        let mut l = String::new();
        if r.read_line(&mut l).unwrap() == 0 || l == "\r\n" {
            break;
        }
    }
    (status, r)
}

fn connect(target: &str) -> String {
    format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n")
}

#[test]
fn each_unit_has_a_listener_of_its_job_with_no_token() {
    let e = egress();
    let d = dir();
    let (a, b) = (d.join("u-1.sock"), d.join("u-2.sock"));
    e.open("u-1", ONLY_A, &a, Duration::from_secs(600)).unwrap();
    e.open("u-2", ONLY_B, &b, Duration::from_secs(600)).unwrap();
    assert_eq!(std::fs::metadata(&a).unwrap().permissions().mode() & 0o777, 0o600);
    // No token: the socket is the job.
    let (status, mut tunnel) = ask(&a, &connect("a.example:443"));
    assert_eq!(status, 200);
    tunnel.get_mut().write_all(b"ping").unwrap();
    let mut got = [0u8; 4];
    tunnel.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"ping");
    // Each listener has the policy of its own job.
    assert_eq!(ask(&a, &connect("b.example:443")).0, 403);
    assert_eq!(ask(&b, &connect("b.example:443")).0, 200);
    // A token of unit A on the listener of unit B is refused.
    let with_a =
        "CONNECT b.example:443 HTTP/1.1\r\nHost: b.example:443\r\nProxy-Authorization: Bearer token-of-u-1\r\n\r\n";
    assert_eq!(ask(&b, with_a).0, 407);
    assert_eq!(Egress::proxy_url(&a), format!("http://unix:{}", a.display()));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_close_and_a_kill_end_the_listeners() {
    let e = egress();
    let d = dir();
    let (a, b) = (d.join("u-1.sock"), d.join("u-2.sock"));
    e.open("u-1", ONLY_A, &a, Duration::from_secs(600)).unwrap();
    e.open("u-2", ONLY_B, &b, Duration::from_secs(600)).unwrap();
    assert!(e.close("u-1"));
    assert!(!a.exists());
    assert!(UnixStream::connect(&a).is_err());
    e.kill();
    assert!(!e.alive());
    assert!(!b.exists());
    assert!(e.open("u-3", ONLY_A, &d.join("u-3.sock"), Duration::from_secs(60)).is_err());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_bad_policy_is_refused_and_named_without_its_text() {
    let e = egress();
    let d = dir();
    let err = e
        .open(
            "u-1",
            r#"{"schema":1,"mode":"allow","allow":[{"host":"secret-host-77"}],"extra":1}"#,
            &d.join("x.sock"),
            Duration::from_secs(60),
        )
        .unwrap_err();
    assert!(!err.0.contains("secret-host-77"), "{err}");
    let _ = std::fs::remove_dir_all(d);
}

/// A CONNECT proxy on the loopback that answers each request with `status`.
fn proxy_that_answers(status: &'static str) -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for conn in l.incoming().flatten() {
            let mut r = BufReader::new(conn.try_clone().unwrap());
            let mut line = String::new();
            while r.read_line(&mut line).is_ok_and(|n| n > 0) && line != "\r\n" {
                line.clear();
            }
            let mut w = conn;
            let _ = w.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n").as_bytes());
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    format!("http://127.0.0.1:{port}")
}

/// T-048: an upstream that takes names is asked with the canary; one that
/// connects to it, and one that refuses it, are told apart.
#[test]
fn the_upstream_check_tells_a_proxy_that_refuses_non_public_names() {
    let takes = pod_proxy::Proxy::parse(&proxy_that_answers("200 Connection established")).unwrap();
    let e = Egress::host(Some(takes), true, Arc::new(super::SystemResolve)).unwrap();
    assert_eq!(e.check_upstream(), "takes-non-public");
    let refuses = pod_proxy::Proxy::parse(&proxy_that_answers("403 Forbidden")).unwrap();
    let e = Egress::host(Some(refuses), true, Arc::new(super::SystemResolve)).unwrap();
    assert_eq!(e.check_upstream(), "refuses-non-public");
}

/// T-048: the load of a policy counts its allow rules under an open suffix.
#[test]
fn an_open_suffix_rule_is_counted() {
    let e = Egress::host(None, false, Arc::new(super::SystemResolve)).unwrap();
    let d = dir();
    let open = r#"{"schema":1,"mode":"allow","allow":[{"host":"*.github.io"},{"host":"github.com"}]}"#;
    assert_eq!(e.open("u-1", open, &d.join("u-1.sock"), Duration::from_secs(60)).unwrap(), 1);
    assert_eq!(e.open("u-2", ONLY_A, &d.join("u-2.sock"), Duration::from_secs(60)).unwrap(), 0);
    e.kill();
}
