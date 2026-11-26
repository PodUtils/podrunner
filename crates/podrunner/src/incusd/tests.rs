use std::io::{Read, Write};
use std::os::unix::net::UnixListener;

use super::*;

/// An incusd that gives `answer` to one request, and the request it read.
fn serve_once(socket: &Path, answer: &'static [u8]) -> std::thread::JoinHandle<String> {
    let listener = UnixListener::bind(socket).unwrap();
    std::thread::spawn(move || {
        let (mut c, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut b = [0u8; 256];
        while !request.ends_with(b"\r\n\r\n") {
            let n = c.read(&mut b).unwrap();
            assert!(n > 0, "the request ends before its head");
            request.extend_from_slice(&b[..n]);
        }
        c.write_all(answer).unwrap();
        String::from_utf8(request).unwrap()
    })
}

const OLD: &[u8] = b"HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n\
{\"type\":\"sync\",\"status_code\":200,\"metadata\":{\"environment\":{\"server_version\":\"6.0.0\",\"server\":\"incus\"}}}\n";

const NEW: &[u8] = b"HTTP/1.0 200 OK\r\n\r\n{\"metadata\":{\"environment\":{\"server_version\":\"6.0.7\"}}}";

#[test]
fn each_incus_before_the_apparmor_fix_gets_the_engine_fault() {
    for v in ["6.0.0", "6.0.5", "6.1", "6.18", "6.18.1", "0.7", "6.0.0-1ubuntu0.3", "5.21.2"] {
        let w = engine_fault(v).unwrap_or_else(|| panic!("{v}: no fault"));
        assert!(w.starts_with(&format!("incusd {v}: ")), "{w}");
        assert!(w.contains("lxc/incus#2623") && w.contains("microvm") && w.contains("runc 1.3.3"), "{w}");
    }
    for v in ["6.0.6", "6.0.7", "6.19", "6.20.1", "7.0", "10.0"] {
        assert_eq!(engine_fault(v), None, "{v}");
    }
    // A version that does not parse gives no guess.
    for v in ["", ".", "..", "abc", "v6.0.0", "${{ x }}", "99999999999.0"] {
        assert_eq!(engine_fault(v), None, "{v:?}");
    }
}

#[test]
fn the_server_version_comes_from_the_answer_of_slash_1_0() {
    assert_eq!(server_version(OLD).unwrap(), "6.0.0");
    assert_eq!(server_version(NEW).unwrap(), "6.0.7");
    let refused = b"HTTP/1.0 403 Forbidden\r\n\r\n{\"error\":\"not authorized\"}";
    assert!(server_version(refused).unwrap_err().contains("403"));
    assert!(server_version(b"").unwrap_err().contains("not HTTP"));
    assert!(server_version(b"HTTP/1.0 200 OK\r\n\r\nnot json").unwrap_err().contains("/1.0"));
    let none = b"HTTP/1.0 200 OK\r\n\r\n{\"metadata\":{\"environment\":{}}}";
    assert!(server_version(none).unwrap_err().contains("no server_version"));
    let empty = b"HTTP/1.0 200 OK\r\n\r\n{\"metadata\":{\"environment\":{\"server_version\":\"\"}}}";
    assert!(server_version(empty).is_err());
}

#[test]
fn the_version_is_read_from_the_socket_of_incusd() {
    let dir = crate::scratch::Scratch::new("incusd");
    let socket = dir.join("unix.socket");
    let server = serve_once(&socket, OLD);
    assert_eq!(version(&socket).unwrap(), "6.0.0");
    assert!(server.join().unwrap().starts_with("GET /1.0 HTTP/1.0\r\n"));
    // With AppArmor on, an old incusd gives the warning, a new one none.
    let apparmor = dir.join("enabled");
    std::fs::write(&apparmor, "Y\n").unwrap();
    let _ = std::fs::remove_file(&socket);
    let server = serve_once(&socket, OLD);
    assert!(engine_check(&socket, &apparmor).unwrap().unwrap().starts_with("incusd 6.0.0: "));
    server.join().unwrap();
    let _ = std::fs::remove_file(&socket);
    let server = serve_once(&socket, NEW);
    assert_eq!(engine_check(&socket, &apparmor), Ok(None));
    server.join().unwrap();
    // No incusd: the check names why; the warning of serve and run is none.
    let _ = std::fs::remove_file(&socket);
    assert!(version(&socket).unwrap_err().starts_with("incusd: "));
    assert!(engine_check(&socket, &apparmor).unwrap_err().starts_with("incusd: "));
    assert_eq!(engine_warning(&socket), None);
    // AppArmor off, or no AppArmor module: no warning, and incusd is not
    // asked (no server answers here).
    for off in ["N\n", "", "n"] {
        std::fs::write(&apparmor, off).unwrap();
        assert_eq!(engine_check(&socket, &apparmor), Ok(None), "{off:?}");
    }
    assert_eq!(engine_check(&socket, &dir.join("absent")), Ok(None));
}
