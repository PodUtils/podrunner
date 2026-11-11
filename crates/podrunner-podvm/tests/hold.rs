//! The egress hold of a podvm unit under `podnet pod` (T-065), on a real
//! podvm and podnet. A CONNECT stand-in on the egress socket of the unit
//! logs each target and tunnels it to a loopback server. The payload and a
//! step through `exec` connect to 10.255.0.1, which no host serves: an
//! answer comes only through the socket. Ignored by default. Run:
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_PODVM_IMAGE=NAME@sha256:HEX PODRUNNER_PODNET=PATH \
//!   cargo test -p podrunner-podvm --test hold -- --ignored
//!
//! The image needs sh, sleep and curl.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use podrunner_podvm::{Podvm, PodvmUnit};
use podrunner_schema::{ContainerRoute, EgressHold, UnitId};
use podrunner_unit::{ContainerSpec, EGRESS_SOCKET, ExecSpec, Exit, StartSpec, UnitDriver};

const BODY: &str = "podrunner-through-the-hold";

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

fn splice(mut a: UnixStream, mut b: TcpStream) {
    let (mut a2, mut b2) = (a.try_clone().unwrap(), b.try_clone().unwrap());
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut b2, &mut a2);
    });
    let _ = std::io::copy(&mut a, &mut b);
}

/// The CONNECT stand-in on `path`: each target goes to `log`, and each
/// tunnel to the loopback port `port`.
fn stand_in(path: &Path, port: u16, log: Arc<Mutex<Vec<String>>>) {
    let l = UnixListener::bind(path).unwrap();
    std::thread::spawn(move || {
        for mut c in l.incoming().flatten() {
            let log = log.clone();
            std::thread::spawn(move || {
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") && head.len() < 8192 {
                    if c.read(&mut byte).unwrap_or(0) == 0 {
                        return;
                    }
                    head.push(byte[0]);
                }
                let line = String::from_utf8_lossy(&head).lines().next().unwrap_or("").to_string();
                log.lock().unwrap().push(line);
                let up = TcpStream::connect(("127.0.0.1", port)).unwrap();
                let _ = c.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n");
                splice(c, up);
            });
        }
    });
}

fn until(mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(30);
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

#[test]
#[ignore]
fn a_held_podvm_unit_sends_each_connect_to_its_egress_socket() {
    let state = std::env::temp_dir().join(format!("podrunner-hold-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let sockets = state.join("sockets");
    std::fs::create_dir_all(&sockets).unwrap();
    let port = server();
    let log = Arc::new(Mutex::new(Vec::new()));
    stand_in(&sockets.join(EGRESS_SOCKET), port, log.clone());

    let podvm = Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) };
    let unit = PodvmUnit::new(podvm, &state.join("podvm"), Path::new(&env("PODRUNNER_PODVM_IMAGES")))
        .unwrap()
        .with_podnet(Path::new(&env("PODRUNNER_PODNET")))
        .unwrap();
    let cap = unit.probe().unwrap();
    assert_eq!(cap.egress_hold, EgressHold::Advisory, "{cap:?}");
    let image = unit.prepare(&env("PODRUNNER_PODVM_IMAGE"), "linux/amd64", false).unwrap();
    let id = UnitId::parse("u-1").unwrap();
    let url = format!("http://10.255.0.1:{port}/");
    let script = format!("curl -s --max-time 10 {url} > podrunner-hold.txt 2>&1; exec sleep 120");
    let spec = StartSpec {
        unit: id.clone(),
        image: image.clone(),
        argv: ["sh", "-c", &script].map(String::from).to_vec(),
        env: Default::default(),
        credential: Default::default(),
        sockets: Some(sockets.clone()),
    };
    unit.start(&spec).unwrap();
    let root = unit.observe(&id).unwrap().unit_root.unwrap();
    let held = until(|| std::fs::read_to_string(root.join("podrunner-hold.txt")).is_ok_and(|t| t.contains(BODY)));

    // A step in a job container: an exec under a podnet pod of its own.
    let stepped = if cap.container == ContainerRoute::Hooks {
        let c = unit
            .container_start(
                &id,
                &ContainerSpec {
                    image: env("PODRUNNER_PODVM_IMAGE"),
                    argv: ["sleep", "120"].map(String::from).to_vec(),
                    ..Default::default()
                },
            )
            .unwrap();
        let out = state.join("step.log");
        let exec = ExecSpec {
            argv: ["curl", "-s", "--max-time", "10", &url].map(String::from).to_vec(),
            log: Some(out.clone()),
            ..Default::default()
        };
        let code = unit.container_exec(&id, &c, &exec).unwrap();
        Some(code == Exit::Code(0) && std::fs::read_to_string(&out).is_ok_and(|t| t.contains(BODY)))
    } else {
        None
    };
    let residue = unit.destroy(&id).unwrap();
    let lines = log.lock().unwrap().clone();
    let _ = std::fs::remove_dir_all(&state);
    assert!(held, "the connect of the payload did not come through the egress socket: {lines:?}");
    assert!(lines.iter().any(|l| l.starts_with(&format!("CONNECT 10.255.0.1:{port}"))), "{lines:?}");
    assert_ne!(stepped, Some(false), "the connect of an exec step did not come through the egress socket");
    assert!(residue.is_clean(), "{residue:?}");
}
