//! T-534: `serve` with an `[unit.incus]` on a host with no incusd starts
//! with no incus kind and one line that names the socket.

use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod scratch;

const BIN: &str = env!("CARGO_BIN_EXE_podrunner");

#[test]
fn serve_with_no_incusd_runs_no_incus_kind() {
    let dir = scratch::Scratch::short("serve-incus");
    std::fs::create_dir_all(dir.join("store")).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let podvm = dir.join("podvm");
    podrunner_unit::program_file::write(
        &podvm,
        "#!/bin/sh\necho '{\"podvm\": \"0.2.0-beta.32\", \"rung\": \"namespace\"}'\n",
    )
    .unwrap();
    std::fs::write(dir.join("script"), "").unwrap();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - 100;
    let config = format!(
        "host = \"host-a\"\nallow_fake = true\n[[source]]\nname = \"local\"\nkind = \"fake\"\ntier = \"local\"\n\
         arch = \"{arch}\"\nscript = \"{script}\"\n[unit.fake]\nslots = 1\n[unit.incus]\nslots = 1\n\
         binary = \"{podvm}\"\nimages = \"{store}\"\n[pin]\n[[pin.payload]]\narch = \"{arch}\"\n\
         reference = \"runner@sha256:{digest}\"\nbuilt_at_s = {now}\n",
        arch = std::env::consts::ARCH,
        script = dir.join("script").display(),
        podvm = podvm.display(),
        store = dir.join("store").display(),
        digest = "1".repeat(64),
    );
    std::fs::write(dir.join("podrunner.toml"), config).unwrap();
    let state = dir.join("state");
    let mut serve = Command::new(BIN)
        .args(["serve", "--config"])
        .arg(dir.join("podrunner.toml"))
        .arg("--state")
        .arg(&state)
        .env("PODVM_INCUS_SOCKET", dir.join("none.socket"))
        .env_remove("NOTIFY_SOCKET")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let status = || Command::new(BIN).args(["status", "--json", "--state"]).arg(&state).output().unwrap();
    let t = Instant::now();
    let v: serde_json::Value = loop {
        let o = status();
        if o.status.success() {
            break serde_json::from_slice(&o.stdout).unwrap();
        }
        assert!(t.elapsed() < Duration::from_secs(20), "serve did not answer");
        assert!(serve.try_wait().unwrap().is_none(), "serve ended");
        std::thread::sleep(Duration::from_millis(100));
    };
    let kinds: Vec<&str> = v["kinds"].as_array().unwrap().iter().filter_map(|k| k["name"].as_str()).collect();
    assert_eq!(kinds, ["fake"], "{v}");
    // SAFETY: kill(2) sends SIGTERM to the child that this test started.
    unsafe { libc::kill(serve.id() as libc::pid_t, libc::SIGTERM) };
    let _ = serve.wait();
    let mut err = String::new();
    serve.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    assert!(err.contains("[unit.incus]: not here: incusd: ") && err.contains("none.socket"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
