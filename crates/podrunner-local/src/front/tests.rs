use std::net::TcpListener;
use std::sync::Mutex;

use super::*;

/// A stand-in of an upstream: it keeps the first line of each request and
/// answers `ok`.
fn upstream_answer(mut c: impl Read + Write, seen: &Mutex<Vec<String>>) {
    let mut buf = [0u8; 4096];
    let n = c.read(&mut buf).unwrap_or(0);
    let text = String::from_utf8_lossy(&buf[..n]).to_string();
    seen.lock().unwrap().push(text.lines().next().unwrap_or("").to_string());
    let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("podrunner-front-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ask(path: &Path, request: &str) -> String {
    let mut c = UnixStream::connect(path).unwrap();
    c.write_all(request.as_bytes()).unwrap();
    let mut out = String::new();
    let _ = c.read_to_string(&mut out);
    out
}

#[test]
fn the_host_of_a_request() {
    assert_eq!(host("CONNECT", "broker.podrunner.test:443"), BROKER);
    assert_eq!(host("GET", "http://Broker.Podrunner.Test:8080/x"), BROKER);
    assert_eq!(host("GET", "http://github.com/a"), "github.com");
    assert_eq!(host("GET", "/origin-form"), "");
}

#[test]
fn the_broker_gets_origin_form_and_another_host_is_refused_with_no_egress() {
    let d = dir("broker");
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = tcp.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    std::thread::spawn(move || {
        for c in tcp.incoming().flatten() {
            upstream_answer(c, &s);
        }
    });
    let front = serve(&d.join("egress.sock"), port, None).unwrap();
    let mode = std::fs::metadata(d.join("egress.sock")).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o666);
    let out =
        ask(&d.join("egress.sock"), &format!("GET http://{BROKER}:{port}/message?x=1 HTTP/1.1\r\nHost: b\r\n\r\n"));
    assert!(out.ends_with("ok"), "{out}");
    assert_eq!(seen.lock().unwrap()[0], "GET /message?x=1 HTTP/1.1");
    let out = ask(&d.join("egress.sock"), "GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n");
    assert!(out.starts_with("HTTP/1.1 403") && out.contains("example.com"), "{out}");
    drop(front);
    assert!(!d.join("egress.sock").exists(), "the drop removes the socket");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn another_host_goes_unchanged_to_the_egress_module() {
    let d = dir("inner");
    let inner = UnixListener::bind(d.join("inner.sock")).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    std::thread::spawn(move || {
        for c in inner.incoming().flatten() {
            upstream_answer(c, &s);
        }
    });
    let _front = serve(&d.join("egress.sock"), 1, Some(d.join("inner.sock"))).unwrap();
    let out = ask(&d.join("egress.sock"), "CONNECT github.com:443 HTTP/1.1\r\nHost: github.com:443\r\n\r\n");
    assert!(out.ends_with("ok"), "{out}");
    assert_eq!(seen.lock().unwrap()[0], "CONNECT github.com:443 HTTP/1.1");
    let _ = std::fs::remove_dir_all(&d);
}
