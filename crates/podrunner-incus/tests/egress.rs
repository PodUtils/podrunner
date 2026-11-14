//! The egress of an instance of the incus kind (T-109): no NIC, and one
//! channel to the egress socket of the unit. A payload reaches a unix
//! socket of the host through its proxy, and nothing directly. Ignored by
//! default: it needs podvm `v0.2.0-beta.12` or later and an incusd.
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_PODVM_IMAGE=runner@sha256:HEX \
//!   cargo test -p podrunner-incus --test egress -- --ignored --nocapture

use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use podrunner_incus::IncusUnit;
use podrunner_podvm::Podvm;
use podrunner_schema::{Credential, UnitId};
use podrunner_unit::{EGRESS_SOCKET, StartSpec, UnitDriver};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[test]
#[ignore]
fn an_instance_reaches_the_egress_socket_and_nothing_else() {
    let state = std::env::temp_dir().join(format!("podrunner-incus-egress-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let sockets = state.join("sockets");
    std::fs::create_dir_all(&sockets).unwrap();
    // The egress socket: each connection gets one answer, and its first
    // line goes to the log.
    let listener = UnixListener::bind(sockets.join(EGRESS_SOCKET)).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    std::thread::spawn(move || {
        for mut c in listener.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let n = c.read(&mut buf).unwrap_or(0);
            let text = String::from_utf8_lossy(&buf[..n]).to_string();
            s.lock().unwrap().push(text.lines().next().unwrap_or("").to_string());
            let _ = c.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 17\r\n\r\nincus-channel-42\n");
        }
    });

    let unit = IncusUnit::new(
        Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) },
        &state,
        &PathBuf::from(env("PODRUNNER_PODVM_IMAGES")),
    );
    let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let image = unit.prepare(&env("PODRUNNER_PODVM_IMAGE"), platform, false).unwrap();
    let script = "echo \"net $(ls /sys/class/net | tr '\\n' ' ')\"; \
        curl -s --max-time 20 http://egress.podrunner.test/ || echo proxied-failed; \
        if curl -s --noproxy '*' --max-time 5 -o /dev/null http://1.1.1.1/; then echo direct-reached; else echo direct-refused; fi; \
        echo end-of-checks; exec sleep 120";
    let id = UnitId::parse("u-1").unwrap();
    let spec = StartSpec {
        unit: id.clone(),
        image,
        argv: vec!["sh".into(), "-c".into(), script.into()],
        env: Default::default(),
        credential: Credential { files: vec![] },
        sockets: Some(sockets.clone()),
        mounts: Vec::new(),
    };
    unit.start(&spec).unwrap();
    // The payload stays up after its checks, so a command can enter it.
    let end = std::time::Instant::now() + Duration::from_secs(300);
    let mut log = String::new();
    while std::time::Instant::now() < end && !log.contains("end-of-checks") {
        std::thread::sleep(Duration::from_secs(1));
        log = String::from_utf8_lossy(&unit.unit_logs(&id).unwrap_or_default()).to_string();
    }
    // T-160: a command of `enter` runs in the instance, with no NIC either.
    let argv = ["sh", "-c", "echo \"in-unit-$((6*7)) $(ls /sys/class/net | tr '\\n' ' ')\""].map(String::from).to_vec();
    let entered = unit.enter(&id, &argv, false).map(|mut c| c.stdin(std::process::Stdio::null()).output());
    let exit = unit.wait(&id, Duration::ZERO).ok();
    let residue = unit.destroy(&id).unwrap();
    let _ = std::fs::remove_dir_all(&state);
    let seen = seen.lock().unwrap().clone();
    println!("exit {exit:?}, residue {residue:?}\n{log}\nthe socket saw: {seen:?}");
    assert!(log.contains("net lo "), "no NIC but lo: {log}");
    assert!(log.contains("incus-channel-42"), "the proxy reaches the egress socket: {log}");
    assert!(log.contains("direct-refused"), "no way out but the socket: {log}");
    assert!(seen.iter().any(|l| l.contains("egress.podrunner.test")), "{seen:?}");
    assert!(residue.is_clean(), "{residue:?}");
    let out = entered.unwrap().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("in-unit-42 lo "), "enter: {text:?} {}", String::from_utf8_lossy(&out.stderr));
}
