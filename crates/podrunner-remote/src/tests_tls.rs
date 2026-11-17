//! The remote mint over mutual TLS on the loopback: a pinned host reaches
//! the gate; a key with no pin, and a mint with another key, fail.

use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pod_tls::mutual::{Pin, Pins, client_config, server_config};
use pod_tls::server::{Served, certified};
use podrunner_schema::JobId;
use podrunner_source::{Event, JobSource, SourceCode, Wire};

use crate::tests::{dir, gate, offer};
use crate::transport::{ALPN, HostKey, Https, Server, provider};

/// A P-256 key and a certificate of it, from openssl; its pin.
fn key(at: &Path, name: &str) -> Option<(Vec<u8>, Vec<u8>, Pin)> {
    let (k, c) = (at.join(format!("{name}.key")), at.join(format!("{name}.pem")));
    let ok = std::process::Command::new("openssl")
        .args(["req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes", "-days", "1"])
        .args(["-subj", &format!("/CN={name}"), "-keyout"])
        .arg(&k)
        .arg("-out")
        .arg(&c)
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        return None;
    }
    let (kp, cp) = (std::fs::read(&k).ok()?, std::fs::read(&c).ok()?);
    let der = pod_tls::server::certificates(&cp).ok()?.remove(0);
    Some((kp, cp, Pin::of_certificate(&der).ok()?))
}

#[test]
fn a_pinned_host_reaches_the_gate_and_another_key_does_not() {
    let d = dir();
    let keys = (key(&d, "mint"), key(&d, "host"), key(&d, "stranger"), key(&d, "old"));
    let (Some(mint), Some(host), Some(stranger), Some(old)) = keys else {
        return; // No openssl on this host.
    };
    let g = Arc::new(Mutex::new(gate(&[offer("j-1", false), offer("j-2", false)], &[("h1", 1)], &d)));
    let served = Served::new(Some(certified(&mint.1, &mint.0).unwrap()));
    let config = server_config(provider(), served, Pins::new([host.2, old.2]), &[ALPN]).unwrap();
    let keys = vec![
        HostKey { pin: host.2, host: "h1".into(), not_after_s: None },
        // The old key of a rotation, past its life.
        HostKey { pin: old.2, host: "h1".into(), not_after_s: Some(1) },
    ];
    let server = Server::new(g, keys, config);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    std::thread::spawn(move || server.serve(listener, Arc::new(AtomicBool::new(false))));

    let client = |k: &(Vec<u8>, Vec<u8>, Pin), mint_pin: Pin| {
        let config = client_config(provider(), certified(&k.1, &k.0).unwrap(), Pins::new([mint_pin]), &[ALPN]).unwrap();
        Wire(Https { address: address.clone(), config, lookup: None })
    };
    let mut h1 = client(&host, mint.2);
    assert_eq!(h1.hello("podrunner", "0").unwrap().source, "src");
    // T-076: a name of the mint that only the lookup knows (`pod-dns`
    // with a pin, in `serve`) reaches the mint.
    let port: u16 = address.rsplit_once(':').unwrap().1.parse().unwrap();
    let lookup: crate::transport::Lookup = Arc::new(|host: &str, port: u16| match host {
        "mint.podrunner.invalid" => Ok(vec![std::net::SocketAddr::from(([127, 0, 0, 1], port))]),
        other => Err(format!("{other}: no pin")),
    });
    let named = client_config(provider(), certified(&host.1, &host.0).unwrap(), Pins::new([mint.2]), &[ALPN]).unwrap();
    let mut by_name =
        Wire(Https { address: format!("mint.podrunner.invalid:{port}"), config: named, lookup: Some(lookup) });
    assert_eq!(by_name.hello("podrunner", "0").unwrap().source, "src");
    let events = h1.next(5, Duration::ZERO).unwrap();
    assert!(matches!(&events[..], [Event::Offer { offer }] if offer.job_id.as_str() == "j-1"), "{events:?}");
    let j = JobId::new("j-1").unwrap();
    h1.ack(&j).unwrap();
    h1.claim(&j, "r").unwrap();
    h1.mint(&j, "r").unwrap();
    assert_eq!(h1.mint(&j, "r").unwrap_err().code, SourceCode::CredentialAlreadyIssued);

    // The old key of the host is past its life.
    assert_eq!(client(&old, mint.2).hello("podrunner", "0").unwrap_err().detail, "the mint: HTTP 403");
    // A key that the mint does not pin fails the handshake.
    let e = client(&stranger, mint.2).hello("podrunner", "0").unwrap_err();
    assert_eq!(e.code, SourceCode::Unavailable);
    // A client that pins another key does not trust the mint.
    let e = client(&host, stranger.2).hello("podrunner", "0").unwrap_err();
    assert_eq!(e.code, SourceCode::Unavailable);
}

/// Each fresh source of the suite is a new gate behind a new mint on the
/// loopback, reached over mutual TLS.
struct OverTls {
    mint: (Vec<u8>, Vec<u8>, Pin),
    host: (Vec<u8>, Vec<u8>, Pin),
}

impl podrunner_source::conformance::Fixture for OverTls {
    fn fresh(&self) -> Box<dyn JobSource> {
        let d = dir();
        let lines = [offer("j-1", false), offer("j-2", false), offer("j-3", false)];
        let g = Arc::new(Mutex::new(gate(&lines, &[("h1", 10)], &d)));
        let served = Served::new(Some(certified(&self.mint.1, &self.mint.0).unwrap()));
        let config = server_config(provider(), served, Pins::new([self.host.2]), &[ALPN]).unwrap();
        let server = Server::new(g, vec![HostKey { pin: self.host.2, host: "h1".into(), not_after_s: None }], config);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        std::thread::spawn(move || server.serve(listener, Arc::new(AtomicBool::new(false))));
        let key = certified(&self.host.1, &self.host.0).unwrap();
        let config = client_config(provider(), key, Pins::new([self.mint.2]), &[ALPN]).unwrap();
        Box::new(Wire(Https { address, config, lookup: None }))
    }
}

#[test]
fn the_source_tests_pass_over_tls() {
    let d = dir();
    let (Some(mint), Some(host)) = (key(&d, "mint"), key(&d, "host")) else {
        return; // No openssl on this host.
    };
    let results = podrunner_source::conformance::run(&OverTls { mint, host }, &[]);
    let failed = podrunner_source::conformance::not_passed(&results);
    assert!(failed.is_empty(), "{results:#?}");
}

/// #246: the mint serves at most `PER_PEER` connections of one peer at one
/// time; one more is closed at once, not given a thread that waits, and a
/// connection of another peer is still served.
#[test]
fn a_flood_of_connections_gets_no_more_threads() {
    use std::io::Read;
    let d = dir();
    let (Some(mint), Some(host)) = (key(&d, "mint"), key(&d, "host")) else {
        return; // No openssl on this host.
    };
    let g = Arc::new(Mutex::new(gate(&[], &[("h1", 1)], &d)));
    let served = Served::new(Some(certified(&mint.1, &mint.0).unwrap()));
    let config = server_config(provider(), served, Pins::new([host.2]), &[ALPN]).unwrap();
    let server = Server::new(g, vec![HostKey { pin: host.2, host: "h1".into(), not_after_s: None }], config);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || server.serve(listener, Arc::new(AtomicBool::new(false))));
    let idle: Vec<std::net::TcpStream> =
        (0..crate::transport::PER_PEER).map(|_| std::net::TcpStream::connect(address).unwrap()).collect();
    std::thread::sleep(std::time::Duration::from_millis(300));
    // A closed connection reads 0 at once; a served one waits for its
    // handshake, so its read ends at its timeout.
    let read = |mut c: std::net::TcpStream| {
        c.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        c.read(&mut [0u8; 16])
    };
    let start = std::time::Instant::now();
    assert_eq!(read(std::net::TcpStream::connect(address).unwrap()).unwrap_or(9), 0, "the 17th of one peer is closed");
    assert!(start.elapsed() < std::time::Duration::from_secs(1), "closed at once: {:?}", start.elapsed());
    // Another address of the loopback is another peer.
    let other = socket2_connect_from([127, 0, 0, 2], address);
    if let Some(other) = other {
        assert!(read(other).is_err(), "another peer is served (its read waits)");
    }
    drop(idle);
}

/// A connection from `from` (an address of the loopback) to `to`, if the
/// host lets this process bind it.
fn socket2_connect_from(from: [u8; 4], to: std::net::SocketAddr) -> Option<std::net::TcpStream> {
    // SAFETY: plain socket calls on a descriptor that this function owns.
    unsafe {
        use std::os::fd::FromRawFd;
        let fd = libc::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0);
        if fd < 0 {
            return None;
        }
        let stream = std::net::TcpStream::from_raw_fd(fd);
        let local = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: 0,
            sin_addr: libc::in_addr { s_addr: u32::from_ne_bytes(from) },
            sin_zero: [0; 8],
        };
        let size = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
        if libc::bind(fd, (&raw const local).cast(), size) != 0 {
            return None;
        }
        let std::net::SocketAddr::V4(v4) = to else { return None };
        let remote = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: v4.port().to_be(),
            sin_addr: libc::in_addr { s_addr: u32::from_ne_bytes(v4.ip().octets()) },
            sin_zero: [0; 8],
        };
        (libc::connect(fd, (&raw const remote).cast(), size) == 0).then_some(stream)
    }
}
