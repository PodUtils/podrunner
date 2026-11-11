//! The official runner in a podvm unit under `podnet pod`, with the egress
//! module of podrunner on the egress socket of the unit (T-065, step 4). A
//! step of the canned job connects to a host that the policy allows and to
//! one that it does not. The broker sends each allowed name to an upstream
//! CONNECT proxy, which logs it and tunnels it to a loopback server. The
//! allowed step gets the body; the denied step fails, and its name never
//! reaches the upstream. Ignored by default. Run:
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_RUNNER_IMAGE=NAME@sha256:HEX PODRUNNER_PODNET=PATH \
//!   cargo test -p podrunner-broker-fake --test held -- --ignored
//!
//! It needs the seccomp hold: under `enforced` the runner does not reach
//! the fake broker on the loopback of the host, so the test skips.
//! The runner gets `DOTNET_SYSTEM_NET_DISABLEIPV6=1`: the seccomp hold
//! does not take `::ffff:127.0.0.1` as loopback (T-077).

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use podrunner_broker_fake::{Broker, Job, files::runner_files};
use podrunner_podvm::{Podvm, PodvmUnit};
use podrunner_schema::{Credential, CredentialFile, EgressHold, Secret, UnitId};
use podrunner_unit::{EGRESS_SOCKET, StartSpec, UnitDriver};

const BODY: &str = "podrunner-allowed-body";
const POLICY: &str = r#"{"schema":1,"mode":"allow","allow":[{"host":"allowed.example"}]}"#;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// A loopback HTTP server that answers each request with BODY.
fn server() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut c in l.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let _ = c.read(&mut buf);
            let _ = write!(c, "HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n{BODY}", BODY.len());
        }
    });
    port
}

/// An upstream CONNECT proxy: each target goes to `log`, and each tunnel
/// to the loopback port `port`.
fn upstream(port: u16, log: Arc<Mutex<Vec<String>>>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for conn in l.incoming().flatten() {
            let log = log.clone();
            std::thread::spawn(move || {
                let mut r = BufReader::new(conn.try_clone().unwrap());
                let mut first = String::new();
                let mut line = String::new();
                let _ = r.read_line(&mut first);
                while r.read_line(&mut line).is_ok_and(|n| n > 0) && line != "\r\n" {
                    line.clear();
                }
                log.lock().unwrap().push(first.trim().to_string());
                let Ok(up) = TcpStream::connect(("127.0.0.1", port)) else { return };
                let mut w = conn;
                let _ = w.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n");
                let (mut up_r, mut up_w) = (up.try_clone().unwrap(), up);
                let mut w2 = w.try_clone().unwrap();
                std::thread::spawn(move || {
                    let _ = std::io::copy(&mut up_r, &mut w2);
                });
                let _ = std::io::copy(&mut r, &mut up_w);
            });
        }
    });
    url
}

#[test]
#[ignore]
fn a_held_runner_reaches_an_allowed_host_and_not_a_denied_one() {
    let state = std::env::temp_dir().join(format!("podrunner-held-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let sockets = state.join("sockets");
    std::fs::create_dir_all(&sockets).unwrap();
    let podvm = Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) };
    let unit = PodvmUnit::new(podvm, &state.join("podvm"), Path::new(&env("PODRUNNER_PODVM_IMAGES")))
        .unwrap()
        .with_podnet(Path::new(&env("PODRUNNER_PODNET")))
        .unwrap();
    let cap = unit.probe().unwrap();
    if cap.egress_hold != EgressHold::Advisory {
        println!("skip: the hold is {:?}; the runner does not reach the broker on the loopback", cap.egress_hold);
        return;
    }

    let port = server();
    let log = Arc::new(Mutex::new(Vec::new()));
    let proxy = pod_proxy::Proxy::parse(&upstream(port, log.clone())).unwrap();
    // The names go to the upstream; the pins only pass the public check.
    let pins: Vec<(String, IpAddr)> =
        ["allowed.example", "denied.example"].map(|h| (h.to_string(), "93.184.215.14".parse().unwrap())).to_vec();
    let egress =
        podrunner_egress::Egress::host(Some(proxy), true, podrunner_egress::dns::resolver(false, pins)).unwrap();
    egress.open("u-1", POLICY, &sockets.join(EGRESS_SOCKET), Duration::from_secs(600)).unwrap();

    let image = env("PODRUNNER_RUNNER_IMAGE");
    let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let prepared = unit.prepare(&image, platform, false).unwrap();
    let broker = Broker::start().unwrap();
    let allowed = format!("curl -s --max-time 20 http://allowed.example:{port}/; echo");
    let denied = format!(
        "if curl -s --max-time 20 http://denied.example:{port}/; then echo denied-$((6*7)); else echo refused-$((6*7)); fi"
    );
    broker.queue(Job::scripts("held", &[&allowed, &denied]));
    let files = runner_files(&broker.base, "runner-u-1", &common::rsaparams())
        .into_iter()
        .map(|(name, text)| CredentialFile { name, content: Secret::new(text) })
        .collect();
    let id = UnitId::parse("u-1").unwrap();
    let spec = StartSpec {
        unit: id.clone(),
        image: prepared,
        argv: vec!["/home/runner/run.sh".into()],
        // The seccomp hold of podnet `v0.4.0-beta.3` sends a dual-stack
        // connect to `::ffff:127.0.0.1` to the egress socket, not direct, so
        // the runner (.NET) would not reach the fake broker on the loopback.
        env: [("DOTNET_SYSTEM_NET_DISABLEIPV6".to_string(), "1".to_string())].into(),
        credential: Credential { files },
        sockets: Some(sockets.clone()),
    };
    unit.start(&spec).unwrap();
    let _ = unit.wait(&id, Duration::from_secs(300)).unwrap();
    let t = Instant::now();
    while broker.completions().is_empty() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let (done, logs) = (broker.completions(), broker.logs());
    // The output of the runner in the unit, for a failure.
    let unit_log = std::process::Command::new(env("PODRUNNER_PODVM"))
        .args(["logs", "u-1"])
        .env("PODVM_STORE", state.join("podvm").join("units").join("u-1").join("store"))
        .output()
        .map(|o| String::from_utf8_lossy(&[o.stdout, o.stderr].concat()).into_owned())
        .unwrap_or_default();
    println!("{unit_log}");
    let residue = unit.destroy(&id).unwrap();
    egress.kill();
    let seen = log.lock().unwrap().clone();
    let _ = std::fs::remove_dir_all(&state);
    assert_eq!(done.len(), 1, "{:?}\n{logs}", broker.seen());
    assert!(logs.contains(BODY), "the allowed step got no body: {logs}");
    // The markers are computed: the log also holds the text of the script.
    assert!(logs.contains("refused-42") && !logs.contains("denied-42"), "{logs}");
    assert!(seen.iter().any(|l| l.starts_with(&format!("CONNECT allowed.example:{port}"))), "{seen:?}");
    assert!(!seen.iter().any(|l| l.contains("denied.example")), "a denied name reached the upstream: {seen:?}");
    assert!(residue.is_clean(), "{residue:?}");
}
