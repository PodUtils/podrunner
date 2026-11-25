//! The front of the egress socket of a unit of `serve` (T-493).

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Ask, Bounds, CONTROL, serve};
use crate::scratch::Scratch;

fn dir() -> Scratch {
    Scratch::new("front")
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
    Arc::new(|route: &str, body: &[u8]| format!("{{\"route\":\"{route}\",\"len\":{}}}", body.len()))
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
    assert!(out.ends_with(&format!("{{\"route\":\"/v1/assigned\",\"len\":{}}}", body.len())), "{out}");
    // The mark of the first step is the other route (T-660).
    let mark = format!("POST http://{CONTROL}/v1/started HTTP/1.1\r\nContent-Length: 2\r\n\r\n{{}}");
    let out = talk(&d.join("egress.sock"), mark.as_bytes());
    assert!(out.starts_with("HTTP/1.1 200 OK") && out.ends_with(r#"{"route":"/v1/started","len":2}"#), "{out}");
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
    // T-660: the mark of the first step is answered at once and names the
    // unit of the socket, whatever its body says.
    assert!(port.first_steps().is_empty());
    let mark = format!("POST http://{CONTROL}/v1/started HTTP/1.1\r\nContent-Length: 14\r\n\r\n{{\"unit\":\"u-9\"}}");
    let out = talk(&socket, mark.as_bytes());
    assert!(out.starts_with("HTTP/1.1 200") && out.ends_with(r#"{"ok":true}"#), "{out}");
    assert_eq!(port.first_steps(), [podrunner_schema::UnitId::parse("u-1").unwrap()]);
    assert!(port.questions().is_empty(), "a mark is no question");
    port.close("u-1");
    assert!(!socket.exists(), "the front goes at the close");
}

/// A second open of a unit keeps its front: the same socket file (a bind
/// mount of an incus instance holds it), and the new policy of the job
/// holds behind it (T-487).
#[test]
fn a_second_open_keeps_the_socket_of_the_unit() {
    use std::os::unix::fs::MetadataExt;

    use podrunner_slots::EgressPort;
    let d = dir();
    let resolve = podrunner_egress::dns::resolver(false, Vec::new());
    let egress = podrunner_egress::Egress::host(None, false, resolve).unwrap();
    let port = super::super::EgressOf::new(egress, Duration::from_secs(60));
    let socket = d.join("egress.sock");
    let only = |h: &str| format!(r#"{{"schema":1,"mode":"allow","allow":[{{"host":"{h}"}}]}}"#);
    port.open("u-1", &only("a.example"), &socket).unwrap();
    let inode = std::fs::metadata(&socket).unwrap().ino();
    port.open("u-1", &only("b.example"), &socket).unwrap();
    let meta = std::fs::metadata(&socket).expect("the socket is there after a second open");
    assert_eq!(meta.ino(), inode, "the same socket file");
    let out = talk(&socket, b"CONNECT c.example:443 HTTP/1.1\r\nHost: c.example:443\r\n\r\n");
    assert!(out.starts_with("HTTP/1.1 403"), "the front still serves: {out}");
    port.close("u-1");
    assert!(!socket.exists(), "the front goes at the close");
}

/// T-487: the socket of a warm unit exists before its job, and a request
/// then leads nowhere (no byte back); the open of the job keeps the same
/// socket, and the policy of the job holds behind it.
#[test]
fn a_held_unit_leads_nowhere_until_the_open_of_its_job() {
    use std::os::unix::fs::MetadataExt;

    use podrunner_slots::EgressPort;
    let d = dir();
    let resolve = podrunner_egress::dns::resolver(false, Vec::new());
    let egress = podrunner_egress::Egress::host(None, false, resolve).unwrap();
    let port = super::super::EgressOf::new(egress, Duration::from_secs(60));
    let socket = d.join("egress.sock");
    port.hold("u-1", &socket).unwrap();
    let inode = std::fs::metadata(&socket).unwrap().ino();
    let connect = b"CONNECT c.example:443 HTTP/1.1\r\nHost: c.example:443\r\n\r\n";
    assert_eq!(talk(&socket, connect), "", "no way out before the open");
    port.open("u-1", r#"{"schema":1,"mode":"allow","allow":[{"host":"a.example"}]}"#, &socket).unwrap();
    assert_eq!(std::fs::metadata(&socket).unwrap().ino(), inode, "the same socket file");
    let out = talk(&socket, connect);
    assert!(out.starts_with("HTTP/1.1 403"), "the policy of the job: {out}");
    port.close("u-1");
    assert!(!socket.exists(), "the front goes at the close");
}

/// The egress port of serve with the units `u-1` and `u-2`, each with its
/// own socket directory.
fn two_units(d: &Scratch) -> (super::super::EgressOf, std::path::PathBuf, std::path::PathBuf) {
    use podrunner_slots::EgressPort;
    let resolve = podrunner_egress::dns::resolver(false, Vec::new());
    let egress = podrunner_egress::Egress::host(None, false, resolve).unwrap();
    let port = super::super::EgressOf::new(egress, Duration::from_secs(60));
    let deny = r#"{"schema":1,"mode":"allow","allow":[{"host":"a.example"}]}"#;
    let (a, b) = (d.join("a/egress.sock"), d.join("b/egress.sock"));
    for (unit, s) in [("u-1", &a), ("u-2", &b)] {
        std::fs::create_dir_all(s.parent().unwrap()).unwrap();
        port.open(unit, deny, s).unwrap();
    }
    (port, a, b)
}

/// T-660: a unit that marks in a loop takes no place of another unit: the
/// one mark of `u-2` arrives, and `u-1` counts once.
#[test]
fn a_unit_that_marks_in_a_loop_loses_no_mark_of_another_unit() {
    use podrunner_slots::EgressPort;
    let d = dir();
    let (port, a, b) = two_units(&d);
    let mark = format!("POST http://{CONTROL}/v1/started HTTP/1.1\r\nContent-Length: 2\r\n\r\n{{}}");
    for _ in 0..200 {
        assert!(talk(&a, mark.as_bytes()).ends_with(r#"{"ok":true}"#));
    }
    assert!(talk(&b, mark.as_bytes()).ends_with(r#"{"ok":true}"#));
    let got: Vec<String> = port.first_steps().iter().map(|u| u.to_string()).collect();
    assert_eq!(got, ["u-1", "u-2"]);
    assert!(port.first_steps().is_empty());
    port.close("u-1");
    port.close("u-2");
}

/// T-660: a unit that asks in a loop has one question in flight; each
/// other gets `question-in-flight` at once, and the one question of `u-2`
/// still reaches the slots.
#[test]
fn a_unit_that_asks_in_a_loop_takes_no_place_of_another_unit() {
    use podrunner_slots::EgressPort;
    let d = dir();
    let (port, a, b) = two_units(&d);
    let ask = |s: std::path::PathBuf| {
        std::thread::spawn(move || {
            let body = r#"{"run_id":2}"#;
            let n = body.len();
            let q = format!("POST http://{CONTROL}/v1/assigned HTTP/1.1\r\nContent-Length: {n}\r\n\r\n{body}");
            talk(&s, q.as_bytes())
        })
    };
    let flood: Vec<_> = (0..100).map(|_| ask(a.clone())).collect();
    let deadline = Instant::now() + Duration::from_secs(10);
    // 36 answers: a shared queue of 64 places would be full now.
    while flood.iter().filter(|t| t.is_finished()).count() < 36 {
        assert!(Instant::now() < deadline, "the flood got no answer");
        std::thread::sleep(Duration::from_millis(20));
    }
    let other = ask(b.clone());
    // The question of u-2 comes while the flood holds its place.
    std::thread::sleep(Duration::from_millis(300));
    let mut seen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !seen.iter().any(|(u, _): &(String, _)| u == "u-2") && Instant::now() < deadline {
        for (unit, _, reply) in port.questions() {
            seen.push((unit.to_string(), reply));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    for (_, reply) in &seen {
        let _ = reply.send(r#"{"verdict":"keep"}"#.into());
    }
    let out = other.join().unwrap();
    assert!(out.ends_with(r#"{"verdict":"keep"}"#), "the question of u-2: {out}");
    let answers: Vec<String> = flood.into_iter().map(|t| t.join().unwrap()).collect();
    let kept = answers.iter().filter(|o| o.ends_with(r#"{"verdict":"keep"}"#)).count();
    let busy = answers.iter().filter(|o| o.ends_with(r#"{"verdict":"end","code":"question-in-flight"}"#)).count();
    assert_eq!((kept, busy), (1, 99), "{answers:?}");
    port.close("u-1");
    port.close("u-2");
}
