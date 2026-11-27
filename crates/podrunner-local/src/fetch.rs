//! The fetch of the actions through the egress module (T-154). git takes
//! no unix socket as its proxy, so a loopback TCP port relays each
//! connection to the socket of one egress grant, whose policy allows the
//! host of the forge only. The way out of the egress module is the proxy
//! that the environment of the host names for that host, or a direct
//! connection. A redirect or a submodule to another host gets 403.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use podrunner_egress::Egress;

/// The name of the grant of the fetch.
const GRANT: &str = "podrunner-run-fetch";

/// A loopback TCP port that relays each connection to a unix socket, until
/// it is dropped.
pub struct Relay {
    pub port: u16,
    stop: Arc<AtomicBool>,
}

fn copy(mut from: impl Read, mut to: impl Write) -> std::io::Result<u64> {
    std::io::copy(&mut from, &mut to)
}

fn pipe(tcp: TcpStream, socket: &Path) {
    let Ok(unix) = UnixStream::connect(socket) else { return };
    let (Ok(tcp2), Ok(unix2)) = (tcp.try_clone(), unix.try_clone()) else { return };
    let up = std::thread::spawn(move || {
        let _ = copy(&tcp2, &unix2);
        let _ = unix2.shutdown(Shutdown::Write);
    });
    let _ = copy(&unix, &tcp);
    let _ = tcp.shutdown(Shutdown::Write);
    let _ = up.join();
}

impl Relay {
    pub fn start(socket: PathBuf) -> std::io::Result<Relay> {
        // No `getsockname` under `podnet pod` (T-228).
        let (listener, port) = podrunner_broker_fake::bind_loopback()?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                if flag.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(c) = conn {
                    let socket = socket.clone();
                    std::thread::spawn(move || pipe(c, &socket));
                }
            }
        });
        Ok(Relay { port, stop })
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the accept loop, so that it sees the flag and ends.
        let _ = TcpStream::connect_timeout(&([127, 0, 0, 1], self.port).into(), Duration::from_secs(1));
    }
}

/// The policy of the grant: the hosts only (`broker-policy.v1`).
pub fn policy(hosts: &[String]) -> String {
    let allow: Vec<serde_json::Value> = hosts.iter().map(|h| serde_json::json!({ "host": h })).collect();
    serde_json::json!({ "schema": 1, "mode": "allow", "allow": allow }).to_string()
}

/// The host of a URL of the forge: `https://HOST[:PORT]/...`.
pub fn host_of(base: &str) -> Option<String> {
    let rest = base.split_once("://")?.1;
    let authority = rest.split('/').next()?;
    let host = authority.rsplit_once('@').map_or(authority, |x| x.1);
    // An IPv6 literal is `[ADDR]` with a port after it (#252): the host of
    // a rule of the egress module is `[ADDR]` in its one spelling.
    if let Some(rest) = host.strip_prefix('[') {
        let ip: std::net::Ipv6Addr = rest.split_once(']')?.0.parse().ok()?;
        return Some(format!("[{ip}]"));
    }
    let host = host.split(':').next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// The way out of an egress module of the local mode: the proxy that the
/// environment names for `host`, else a direct connection (T-233). On a
/// host whose only way out is a proxy, a direct connection gets nothing.
pub fn env_proxy(host: &str) -> Result<Option<pod_proxy::Proxy>, String> {
    let env = pod_proxy::Environment::read(&|n| std::env::var_os(n));
    env.proxy_for(host, &[]).map_err(|e| format!("the proxy of the environment: {e}"))
}

/// One grant of the egress module for a fetch, and its relay. Drop it
/// after the fetch: the grant, its socket and the module end.
pub struct Fetch {
    egress: Egress,
    _relay: Relay,
    dir: PathBuf,
    /// The proxy URL for git: `http://127.0.0.1:PORT`.
    pub proxy: String,
}

impl Fetch {
    /// Open a grant that allows `host` only.
    pub fn open(host: &str) -> Result<Fetch, String> {
        let upstream = env_proxy(host)?;
        let names = upstream.is_some();
        let resolve = podrunner_egress::dns::resolver_via(upstream.as_ref());
        // A failed way out falls back to the proxy: no line (T-331 F8).
        let egress = Egress::host(upstream, names, resolve).map_err(|e| format!("the egress module: {e}"))?.quiet();
        let dir = std::env::temp_dir().join(format!("podrunner-run-fetch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let socket = dir.join("egress.sock");
        let fail = |e: String, egress: &Egress| {
            egress.kill();
            let _ = std::fs::remove_dir_all(&dir);
            e
        };
        if let Err(e) = egress.open(GRANT, &policy(&[host.to_string()]), &socket, Duration::from_secs(3600)) {
            return Err(fail(format!("the egress grant of the fetch: {e}"), &egress));
        }
        let relay = match Relay::start(socket) {
            Ok(r) => r,
            Err(e) => return Err(fail(format!("the relay of the fetch: {}", e.kind()), &egress)),
        };
        let proxy = format!("http://127.0.0.1:{}", relay.port);
        Ok(Fetch { egress, _relay: relay, dir, proxy })
    }
}

impl Drop for Fetch {
    fn drop(&mut self) {
        self.egress.close(GRANT);
        self.egress.kill();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_relay_carries_bytes_both_ways_and_the_host_of_a_forge() {
        let dir = crate::tests::ShortDir::new("relay");
        let socket = dir.sock("s.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        // An echo server on the socket: it sends back what it reads, in upper case.
        std::thread::spawn(move || {
            let (mut c, _) = listener.accept().unwrap();
            let mut buf = String::new();
            c.read_to_string(&mut buf).unwrap();
            c.write_all(buf.to_uppercase().as_bytes()).unwrap();
        });
        let relay = Relay::start(socket).unwrap();
        let mut t = TcpStream::connect(("127.0.0.1", relay.port)).unwrap();
        t.write_all(b"connect github.com").unwrap();
        t.shutdown(Shutdown::Write).unwrap();
        let mut back = String::new();
        t.read_to_string(&mut back).unwrap();
        assert_eq!(back, "CONNECT GITHUB.COM");
        drop(relay);
        assert_eq!(host_of("https://github.com/").as_deref(), Some("github.com"));
        assert_eq!(host_of("https://u@Git.Example:8443/org/").as_deref(), Some("git.example"));
        assert_eq!(host_of("github.com"), None);
        // #252: an IPv6 literal in brackets, with a port and a user part.
        assert_eq!(host_of("https://[::1]:8443/").as_deref(), Some("[::1]"));
        assert_eq!(host_of("https://[2001:DB8::1]/x").as_deref(), Some("[2001:db8::1]"));
        assert_eq!(host_of("https://u@[::1]:8443/").as_deref(), Some("[::1]"));
        assert_eq!(host_of("https://[::1/").as_deref(), None, "no closing bracket");
        assert_eq!(host_of("https://[]/").as_deref(), None);
        assert!(policy(&["github.com".into()]).contains("\"host\":\"github.com\""));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
