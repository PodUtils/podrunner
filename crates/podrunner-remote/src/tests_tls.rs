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
use crate::transport::{ALPN, Https, Server, provider};

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
    let (Some(mint), Some(host), Some(stranger)) = (key(&d, "mint"), key(&d, "host"), key(&d, "stranger")) else {
        return; // No openssl on this host.
    };
    let g = Arc::new(Mutex::new(gate(&[offer("j-1", false), offer("j-2", false)], &[("h1", 1)], &d)));
    let served = Served::new(Some(certified(&mint.1, &mint.0).unwrap()));
    let config = server_config(provider(), served, Pins::new([host.2]), &[ALPN]).unwrap();
    let server = Server::new(g, vec![(host.2, "h1".into())], config);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    std::thread::spawn(move || server.serve(listener, Arc::new(AtomicBool::new(false))));

    let client = |k: &(Vec<u8>, Vec<u8>, Pin), mint_pin: Pin| {
        let config = client_config(provider(), certified(&k.1, &k.0).unwrap(), Pins::new([mint_pin]), &[ALPN]).unwrap();
        Wire(Https { address: address.clone(), config })
    };
    let mut h1 = client(&host, mint.2);
    assert_eq!(h1.hello("podrunner", "0").unwrap().source, "src");
    let events = h1.next(5, Duration::ZERO).unwrap();
    assert!(matches!(&events[..], [Event::Offer { offer }] if offer.job_id.as_str() == "j-1"), "{events:?}");
    let j = JobId::new("j-1").unwrap();
    h1.ack(&j).unwrap();
    h1.claim(&j, "r").unwrap();
    h1.mint(&j, "r").unwrap();
    assert_eq!(h1.mint(&j, "r").unwrap_err().code, SourceCode::CredentialAlreadyIssued);

    // A key that the mint does not pin fails the handshake.
    let e = client(&stranger, mint.2).hello("podrunner", "0").unwrap_err();
    assert_eq!(e.code, SourceCode::Unavailable);
    // A client that pins another key does not trust the mint.
    let e = client(&host, stranger.2).hello("podrunner", "0").unwrap_err();
    assert_eq!(e.code, SourceCode::Unavailable);
}
