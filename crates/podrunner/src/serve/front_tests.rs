//! The front of the egress socket of a unit of `serve` (T-493).

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Ask, Bounds, CONTROL, serve};

fn dir() -> PathBuf {
    let d = std::env::temp_dir()
        .join(format!("podrunner-front-{}-{:?}", std::process::id(), std::thread::current().id()))
        .join("x");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// An egress module that answers each connection with its first line.
fn inner(path: &std::path::Path) {
    let l = UnixListener::bind(path).unwrap();
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            let mut first = String::new();
            let _ = BufReader::new(c.try_clone().unwrap()).read_line(&mut first);
            let _ = (&c).write_all(format!("inner saw {first}").as_bytes());
        }
    });
}

fn echo_ask() -> Ask {
    Arc::new(|body: &[u8]| format!("{{\"len\":{}}}", body.len()))
}

fn talk(path: &std::path::Path, bytes: &[u8]) -> String {
    let mut c = UnixStream::connect(path).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    c.write_all(bytes).unwrap();
    let mut out = String::new();
    let _ = c.read_to_string(&mut out);
    out
}

#[test]
fn a_question_is_answered_and_each_other_request_goes_to_the_module() {
    let d = dir();
    inner(&d.join("inner.sock"));
    let _f = serve(&d.join("egress.sock"), d.join("inner.sock"), echo_ask(), Bounds::default()).unwrap();
    let body = r#"{"run_id":2}"#;
    let q = format!(
        "POST http://{CONTROL}/v1/assigned HTTP/1.1\r\nHost: {CONTROL}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let out = talk(&d.join("egress.sock"), q.as_bytes());
    assert!(out.starts_with("HTTP/1.1 200 OK"), "{out}");
    assert!(out.ends_with(&format!("{{\"len\":{}}}", body.len())), "{out}");
    // Another path of the control host is refused; so is a CONNECT to it.
    let other = format!("POST http://{CONTROL}/v1/other HTTP/1.1\r\nContent-Length: 0\r\n\r\n");
    assert!(talk(&d.join("egress.sock"), other.as_bytes()).starts_with("HTTP/1.1 404"));
    let tunnel = format!("CONNECT {CONTROL}:443 HTTP/1.1\r\n\r\n");
    assert!(talk(&d.join("egress.sock"), tunnel.as_bytes()).starts_with("HTTP/1.1 404"));
    // A body with no length is refused.
    let no_len = format!("POST http://{CONTROL}/v1/assigned HTTP/1.1\r\n\r\n");
    assert!(talk(&d.join("egress.sock"), no_len.as_bytes()).starts_with("HTTP/1.1 411"));
    let out = talk(&d.join("egress.sock"), b"CONNECT github.com:443 HTTP/1.1\r\nHost: github.com\r\n\r\n");
    assert_eq!(out, "inner saw CONNECT github.com:443 HTTP/1.1\r\n");
}

#[test]
fn a_long_line_a_silent_peer_and_too_many_peers_are_bounded() {
    let d = dir();
    inner(&d.join("inner.sock"));
    let bounds = Bounds { head: Duration::from_secs(1), conns: 2 };
    let _f = serve(&d.join("egress.sock"), d.join("inner.sock"), echo_ask(), bounds).unwrap();
    // A line of 70 KiB with no end of line.
    let out = talk(&d.join("egress.sock"), &vec![b'a'; 70 * 1024]);
    assert!(out.starts_with("HTTP/1.1 431"), "{}", &out[..out.len().min(80)]);
    // Two silent peers hold the two places; the third gets 503 at once.
    let silent: Vec<UnixStream> = (0..2).map(|_| UnixStream::connect(d.join("egress.sock")).unwrap()).collect();
    // The front takes them in its own time: wait for the answer, with a bound.
    let deadline = Instant::now() + Duration::from_secs(5);
    let full = loop {
        let out = talk(&d.join("egress.sock"), b"");
        if out.starts_with("HTTP/1.1 503") || Instant::now() > deadline {
            break out;
        }
    };
    assert!(full.starts_with("HTTP/1.1 503"), "{full}");
    // A silent peer ends after the time of a head.
    let t = Instant::now();
    let mut out = String::new();
    let mut s = silent.into_iter().next().unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let _ = s.read_to_string(&mut out);
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    assert!(out.starts_with("HTTP/1.1 431") || out.is_empty(), "{out}");
}

/// The egress port of serve: the unit sees the front; a question reaches
/// `questions` and its answer goes back; a request for another host goes
/// to the egress module, whose policy refuses it.
#[test]
fn the_egress_port_of_serve_carries_the_questions_of_a_unit() {
    use podrunner_slots::EgressPort;
    let d = dir();
    let resolve = podrunner_egress::dns::resolver(false, Vec::new());
    let egress = podrunner_egress::Egress::host(None, false, resolve).unwrap();
    let port = super::super::EgressOf::new(egress, Duration::from_secs(60));
    let socket = d.join("egress.sock");
    let deny = r#"{"schema":1,"mode":"allow","allow":[{"host":"a.example"}]}"#;
    port.open("u-1", deny, &socket).unwrap();
    let s = socket.clone();
    let asker = std::thread::spawn(move || {
        let body = r#"{"run_id":2}"#;
        let q = format!("POST http://{CONTROL}/v1/assigned HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}", body.len());
        talk(&s, q.as_bytes())
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let (unit, body, reply) = loop {
        if let Some(q) = port.questions().pop() {
            break q;
        }
        assert!(Instant::now() < deadline, "no question");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!((unit.as_str(), body.as_slice()), ("u-1", br#"{"run_id":2}"#.as_slice()));
    reply.send(r#"{"verdict":"keep"}"#.into()).unwrap();
    let out = asker.join().unwrap();
    assert!(out.starts_with("HTTP/1.1 200") && out.ends_with(r#"{"verdict":"keep"}"#), "{out}");
    let out = talk(&socket, b"CONNECT b.example:443 HTTP/1.1\r\nHost: b.example:443\r\n\r\n");
    assert!(out.starts_with("HTTP/1.1 403"), "the policy of the module: {out}");
    port.close("u-1");
    assert!(!socket.exists(), "the front goes at the close");
}
