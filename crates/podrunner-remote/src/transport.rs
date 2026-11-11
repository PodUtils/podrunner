//! The transport of the remote mint (GitHub #31): `POST /v1/source` with
//! one request line of the source wire, over mutual TLS with pinned keys
//! (`pod_tls::mutual`): the key of the client names its host, and the
//! client trusts only the pinned key of the mint. One request on each
//! connection; a body of at most 1 MiB; a rate limit for each host.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pod_tls::mutual::{Pin, peer_pin};
use podrunner_hook::http::{read_request, write_response};
use podrunner_source::wire::LINE_MAX;
use podrunner_source::{Exchange, SourceCode, SourceError};
use rustls::pki_types::ServerName;

use crate::gate::Gate;

/// The ALPN of the remote mint.
pub const ALPN: &[u8] = b"podrunner-mint/1";
/// The most calls of one host in [`WINDOW`].
pub const RATE: u32 = 120;
pub const WINDOW: Duration = Duration::from_secs(10);
/// The longest wait for a read: above the long poll of `next`.
const READ_TIMEOUT: Duration = Duration::from_secs(90);

/// The provider of both sides.
pub fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// A pinned key of a host: its host, and the end of its life in seconds of
/// the Unix epoch (none: no end). A rotation pins the new key beside the
/// old one, then removes the old one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKey {
    pub pin: Pin,
    pub host: String,
    pub not_after_s: Option<u64>,
}

/// The mint side: the gate, and the host of each pinned key.
pub struct Server {
    pub gate: Arc<Mutex<Gate>>,
    pub hosts: Vec<HostKey>,
    pub config: Arc<rustls::ServerConfig>,
    calls: Mutex<HashMap<String, (Instant, u32)>>,
}

impl Server {
    pub fn new(gate: Arc<Mutex<Gate>>, hosts: Vec<HostKey>, config: Arc<rustls::ServerConfig>) -> Arc<Server> {
        Arc::new(Server { gate, hosts, config, calls: Mutex::default() })
    }

    /// False when `host` made [`RATE`] calls in this window.
    fn admit(&self, host: &str) -> bool {
        let mut calls = self.calls.lock().expect("the rates");
        let now = Instant::now();
        let e = calls.entry(host.into()).or_insert((now, 0));
        if now.duration_since(e.0) >= WINDOW {
            *e = (now, 0);
        }
        e.1 += 1;
        e.1 <= RATE
    }

    /// Serve until `stop`: one thread for each connection.
    pub fn serve(self: Arc<Self>, listener: TcpListener, stop: Arc<AtomicBool>) {
        for conn in listener.incoming() {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            let Ok(tcp) = conn else { continue };
            let me = self.clone();
            std::thread::spawn(move || me.one(tcp));
        }
    }

    fn one(&self, tcp: TcpStream) {
        let peer = tcp.peer_addr().map(|a| a.to_string()).unwrap_or_default();
        let _ = tcp.set_read_timeout(Some(READ_TIMEOUT));
        let Ok(conn) = rustls::ServerConnection::new(self.config.clone()) else { return };
        let mut tls = rustls::StreamOwned::new(conn, tcp);
        // The handshake runs in the first read; a key with no pin fails it.
        let req = match read_request(&mut BufReader::new(&mut tls)) {
            Ok(r) => r,
            Err(e) => {
                let _ = write_response(&mut tls, e.0, &[], e.1.as_bytes());
                return;
            }
        };
        let pin = peer_pin(tls.conn.peer_certificates());
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let key = pin.and_then(|p| self.hosts.iter().find(|k| k.pin == p));
        let live = key.filter(|k| k.not_after_s.is_none_or(|t| now < t));
        let host = live.map(|k| k.host.clone());
        let (status, body) = match (host, req.method.as_str(), req.path.as_str()) {
            (None, _, _) if key.is_some() => (403, "the key of the host is past its life\n".to_string()),
            (None, _, _) => (403, "no host of this key\n".to_string()),
            (Some(_), m, p) if m != "POST" || p != "/v1/source" => (404, "no such route\n".to_string()),
            (Some(h), _, _) if !self.admit(&h) => (429, "too many calls\n".to_string()),
            (Some(h), _, _) => match self.gate.lock() {
                Ok(mut g) => (200, g.line(&h, &peer, &req.body)),
                Err(_) => (500, "the gate is lost\n".to_string()),
            },
        };
        let _ = write_response(&mut tls, status, &[], body.as_bytes());
        tls.conn.send_close_notify();
        let _ = tls.flush();
    }
}

/// The unit host side: the wire to the mint at `address` (`host:port`).
pub struct Https {
    pub address: String,
    pub config: Arc<rustls::ClientConfig>,
}

fn down(what: &str) -> SourceError {
    SourceError::new(SourceCode::Unavailable, format!("the mint: {what}"))
}

impl Exchange for Https {
    fn exchange(&mut self, line: &str) -> Result<Vec<u8>, SourceError> {
        let io = |e: std::io::Error| down(&format!("{}", e.kind()));
        let tcp = TcpStream::connect(&self.address).map_err(io)?;
        tcp.set_read_timeout(Some(READ_TIMEOUT)).map_err(io)?;
        // The pin names the mint; the name of the connect is not checked.
        let name = ServerName::try_from("podrunner-mint").map_err(|_| down("a server name"))?;
        let conn = rustls::ClientConnection::new(self.config.clone(), name).map_err(|_| down("the TLS client"))?;
        let mut tls = rustls::StreamOwned::new(conn, tcp);
        let head =
            format!("POST /v1/source HTTP/1.1\r\nHost: podrunner-mint\r\nContent-Length: {}\r\n\r\n", line.len());
        tls.write_all(head.as_bytes())
            .and_then(|()| tls.write_all(line.as_bytes()))
            .and_then(|()| tls.flush())
            .map_err(io)?;
        let mut r = BufReader::new(tls);
        let mut status = String::new();
        r.read_line(&mut status).map_err(io)?;
        let code = status.split(' ').nth(1).unwrap_or("");
        let mut length = None;
        loop {
            let mut h = String::new();
            if r.read_line(&mut h).map_err(io)? == 0 || h.trim().is_empty() {
                break;
            }
            if let Some((k, v)) = h.split_once(':')
                && k.trim().eq_ignore_ascii_case("content-length")
            {
                length = v.trim().parse::<usize>().ok();
            }
        }
        let length = length.filter(|n| *n <= LINE_MAX + 1).ok_or_else(|| down("a reply with no length"))?;
        let mut body = vec![0; length];
        r.read_exact(&mut body).map_err(io)?;
        if code != "200" {
            return Err(down(&format!("HTTP {code}")));
        }
        Ok(body)
    }
}
