//! T-629: a host with no `/dev/kvm` cannot run the microvm kind. `config
//! init` refuses it with the reason, `doctor --config` says that it is not
//! served here and why, `check` and `serve` count it out, and `serve`
//! starts no self-test of a guest (and podvm fetches no QEMU) for it.
//! On a host where `/dev/kvm` opens, each test checks the other side.
//! Also `stop --wait` before `serve` has its control socket.

use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use scratch::Scratch;

mod scratch;

const BIN: &str = env!("CARGO_BIN_EXE_podrunner");

/// The probe of the doctor: `/dev/kvm` opens for reading and writing.
fn kvm() -> bool {
    std::fs::OpenOptions::new().read(true).write(true).open("/dev/kvm").is_ok()
}

/// A stand-in of podvm. A run of a verb other than `probe` leaves a mark,
/// so a test sees that `serve` started no self-test.
fn podvm(dir: &Path) -> String {
    let p = dir.join("podvm");
    let script = format!(
        "#!/bin/sh\ncase \"$1\" in probe) ;; *) echo \"$1\" >> {} ;; esac\n\
         echo '{{\"podvm\": \"0.2.0-beta.36\", \"rung\": \"namespace\"}}'\n",
        dir.join("verbs").display()
    );
    std::fs::write(&p, script).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p.display().to_string()
}

/// A file with a fake source of the local tier and the kinds `units`.
fn config(dir: &Path, units: &str) -> std::path::PathBuf {
    std::fs::create_dir_all(dir.join("store")).unwrap();
    std::fs::write(dir.join("script"), "").unwrap();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - 100;
    let text = format!(
        "host = \"host-a\"\nallow_fake = true\nguest_kinds = [\"microvm\"]\n[[source]]\nname = \"local\"\n\
         kind = \"fake\"\ntier = \"local\"\narch = \"{arch}\"\nscript = \"{script}\"\n{units}\
         [pin]\n[[pin.payload]]\narch = \"{arch}\"\nreference = \"runner@sha256:{digest}\"\nbuilt_at_s = {now}\n",
        arch = std::env::consts::ARCH,
        script = dir.join("script").display(),
        digest = "1".repeat(64),
    );
    let f = dir.join("podrunner.toml");
    std::fs::write(&f, text).unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600)).unwrap();
    f
}

fn microvm(dir: &Path) -> String {
    format!("[unit.microvm]\nslots = 1\nbinary = \"{}\"\nimages = \"{}\"\n", podvm(dir), dir.join("store").display())
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn config_init_refuses_the_microvm_kind_with_no_kvm() {
    let dir = Scratch::new("kvm-init");
    std::fs::create_dir_all(dir.join("store")).unwrap();
    let p = podvm(&dir);
    let init = |kinds: &str, out: &str| {
        Command::new(BIN)
            .args(["config", "init", "--host", "host-a", "--source", "s", "--program", "/bin/true"])
            .args(["--org", "example-org", "--podvm", &p, "--images"])
            .arg(dir.join("store"))
            .args(["--image", &format!("runner@sha256:{}", "1".repeat(64)), "--built-at-s", "1793491200"])
            .args(["--kinds", kinds, "--json", "--out"])
            .arg(dir.join(out))
            .env("PODVM_INCUS_SOCKET", dir.join("incus.socket"))
            .output()
            .unwrap()
    };
    let only = init("microvm", "a.toml");
    let both = init("microvm,podvm", "c.toml");
    // An incusd that takes a connection: the incus kind serves the source.
    let _incusd = std::os::unix::net::UnixListener::bind(dir.join("incus.socket")).unwrap();
    let auto = init("auto", "b.toml");
    if kvm() {
        assert_eq!(only.status.code(), Some(0), "{}", text(&only));
        return;
    }
    // The kind alone: no file, 78, and the reason names /dev/kvm.
    assert_eq!(only.status.code(), Some(78), "{}", text(&only));
    assert!(text(&only).contains("microvm: /dev/kvm: "), "{}", text(&only));
    assert!(!dir.join("a.toml").exists());
    // check refuses the podvm kind alone (no hold), and the text names the
    // kind that the probe refused.
    assert_eq!(both.status.code(), Some(78), "{}", text(&both));
    assert!(text(&both).contains("no-tier-served") && text(&both).contains("; kind microvm: not here: /dev/kvm: "));
    // auto: the kind is in `refused` with its reason, and not in the file.
    let v: serde_json::Value = match auto.status.code() {
        Some(0) => serde_json::from_slice(&auto.stdout).unwrap(),
        _ => panic!("{}", text(&auto)),
    };
    assert!(!v["kinds"].as_array().unwrap().contains(&"microvm".into()), "{v}");
    let why = v["refused"].as_array().unwrap().iter().find(|r| r["kind"] == "microvm").map(|r| r["why"].clone());
    assert!(why.as_ref().and_then(|w| w.as_str()).is_some_and(|w| w.starts_with("/dev/kvm: ")), "{v}");
    assert!(!std::fs::read_to_string(dir.join("b.toml")).unwrap().contains("[unit.microvm]"));
}

#[test]
fn doctor_and_check_count_out_the_microvm_kind_with_no_kvm() {
    let dir = Scratch::new("kvm-check");
    let f = config(&dir, &microvm(&dir));
    let doctor = Command::new(BIN).args(["doctor", "--config"]).arg(&f).output().unwrap();
    let check = Command::new(BIN).args(["check", "--config"]).arg(&f).output().unwrap();
    if kvm() {
        assert!(text(&doctor).contains("kind microvm: boundary guest"), "{}", text(&doctor));
        assert_eq!(check.status.code(), Some(0), "{}", text(&check));
        let here = Command::new(BIN).args(["check", "--here", "--config"]).arg(&f).output().unwrap();
        assert_eq!(here.status.code(), Some(0), "{}", text(&here));
        return;
    }
    let d = text(&doctor);
    assert!(d.contains("kind microvm: not served on this host: /dev/kvm: "), "{d}");
    assert!(!d.contains("kind microvm: boundary"), "{d}");
    assert!(d.contains("source local: tier local, not served"), "{d}");
    assert!(d.contains("(no unit kind of x86_64 runs on this host)") || std::env::consts::ARCH != "x86_64", "{d}");
    // check gives the verdict of each host (a build machine validates the
    // file of a serve host): the kind counts, and its line is on stderr.
    assert_eq!(check.status.code(), Some(0), "{}", text(&check));
    assert!(String::from_utf8_lossy(&check.stderr).contains("podrunner: [unit.microvm]: not here: /dev/kvm: "));
    // check --here and serve refuse the file: no other kind serves it. serve
    // ends with 75, not 78: the kind can come later (udev), and a supervisor
    // starts serve again.
    let here = Command::new(BIN).args(["check", "--here", "--config"]).arg(&f).output().unwrap();
    assert_eq!(here.status.code(), Some(75), "{}", text(&here));
    assert!(text(&here).contains("no-tier-served") && text(&here).contains("[unit.microvm] not here: /dev/kvm: "));
    let state = dir.join("state");
    let t = Instant::now();
    let serve = Command::new(BIN).args(["serve", "--config"]).arg(&f).arg("--state").arg(&state).output().unwrap();
    assert_eq!((serve.status.code(), text(&serve)), (Some(75), text(&here)));
    assert!(t.elapsed() < Duration::from_secs(10));
    assert!(!dir.join("verbs").exists(), "{:?}", std::fs::read_to_string(dir.join("verbs")));
}

/// A file with the fake kind too: check takes it and names the kind that
/// it counts out; serve runs the fake kind only, starts no self-test of
/// a guest and says why in one line.
#[test]
fn serve_runs_the_other_kinds_and_names_the_microvm_kind_with_no_kvm() {
    if kvm() {
        return;
    }
    let dir = Scratch::new("kvm-serve");
    let f = config(&dir, &format!("[unit.fake]\nslots = 1\n{}", microvm(&dir)));
    let check = Command::new(BIN).args(["check", "--config"]).arg(&f).output().unwrap();
    assert_eq!(check.status.code(), Some(0), "{}", text(&check));
    assert!(text(&check).contains("podrunner: [unit.microvm]: not here: /dev/kvm: "), "{}", text(&check));
    let state = dir.join("state");
    let mut serve = Command::new(BIN)
        .args(["serve", "--config"])
        .arg(&f)
        .arg("--state")
        .arg(&state)
        .env_remove("NOTIFY_SOCKET")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let t = Instant::now();
    let v: serde_json::Value = loop {
        let o = Command::new(BIN).args(["status", "--json", "--state"]).arg(&state).output().unwrap();
        if o.status.success() {
            break serde_json::from_slice(&o.stdout).unwrap();
        }
        assert!(t.elapsed() < Duration::from_secs(20), "serve did not answer");
        assert!(serve.try_wait().unwrap().is_none(), "serve ended");
        std::thread::sleep(Duration::from_millis(100));
    };
    // SAFETY: kill(2) sends SIGTERM to the child that this test started.
    unsafe { libc::kill(serve.id() as libc::pid_t, libc::SIGTERM) };
    let _ = serve.wait();
    let mut err = String::new();
    serve.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    let kinds: Vec<&str> = v["kinds"].as_array().unwrap().iter().filter_map(|k| k["name"].as_str()).collect();
    assert_eq!(kinds, ["fake"], "{v}");
    assert_eq!(err.matches("[unit.microvm]: not here: /dev/kvm: ").count(), 1, "{err}");
    assert!(!err.contains("the claim of guest"), "{err}");
    assert!(!dir.join("verbs").exists(), "{:?}", std::fs::read_to_string(dir.join("verbs")));
}

/// `stop --wait` while a serve holds the lock of the state directory and
/// has no control socket yet (its start): the stop does not reach it, and
/// the timeout says so (69, a control socket that does not answer), not
/// "a job is not final". With no serve, the answer is at once.
#[test]
fn stop_wait_before_the_control_socket_says_so() {
    use std::os::fd::AsRawFd;
    let dir = Scratch::new("stop-early");
    let none = Command::new(BIN).args(["stop", "--wait", "--timeout", "1", "--state"]).arg(&*dir).output().unwrap();
    assert_eq!(none.status.code(), Some(0), "{}", text(&none));
    assert!(text(&none).contains("no serve runs"), "{}", text(&none));
    let lock = std::fs::File::create(dir.join("serve.lock")).unwrap();
    // SAFETY: flock(2) on a file that this test holds open.
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) }, 0);
    let held = Command::new(BIN).args(["stop", "--wait", "--timeout", "1", "--state"]).arg(&*dir).output().unwrap();
    assert_eq!(held.status.code(), Some(69), "{}", text(&held));
    assert!(text(&held).contains("serve has no control socket yet"), "{}", text(&held));
    assert!(!text(&held).contains("a job is not final"), "{}", text(&held));
}
