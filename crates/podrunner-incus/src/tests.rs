use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU32, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as STANDARD_DECODE;
use podrunner_unit::conformance::{CANARY, canary_credential};

use super::*;

const IMAGE: &str = "runner@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A fake `podvm` of the incus tier: the state of each instance is a file
/// in `PODVM_STORE`. `run -d -i` keeps stdin and argv and runs BODY in the
/// background; `inspect`, `stop`, `rm -f` and `logs` act on that state.
const FAKE: &str = r#"#!/bin/sh
S=$PODVM_STORE; mkdir -p "$S"
verb=$1; shift
for n; do :; done
case "$verb" in
run)
  name=$4
  cat > "$S/$name.stdin"
  printf '%s\n' "$@" > "$S/$name.argv"
  echo "Running 0" > "$S/$name.status"
  ( sh -c "$BODY" > "$S/$name.log" 2>&1; echo "Stopped $?" > "$S/$name.status" ) </dev/null >/dev/null 2>&1 &
  echo $! > "$S/$name.pid"
  echo "$name" ;;
inspect) [ -f "$S/$n.status" ] || { echo "no instance $n" >&2; exit 1; }; cat "$S/$n.status" ;;
stop|rm)
  [ -f "$S/$n.pid" ] && { pkill -P "$(cat "$S/$n.pid")"; kill "$(cat "$S/$n.pid")"; } 2>/dev/null
  if [ "$verb" = rm ]; then rm -f "$S/$n".*; else echo "Stopped 143" > "$S/$n.status"; fi ;;
logs) cat "$S/$n.log" ;;
esac
"#;

/// A fresh directory of the test.
fn dir(name: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "podrunner-incus-{name}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// A driver over the fake `podvm`, whose instances run `body`.
fn driver(name: &str, body: &str) -> (IncusUnit, PathBuf) {
    let d = dir(name);
    let fake = d.join("podvm");
    fs::write(&fake, FAKE.replace("\"$BODY\"", &format!("'{body}'"))).unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let u = IncusUnit::new(Podvm { binary: fake }, &d.join("state"), &d.join("store"));
    u.prepare(IMAGE, &u.platform.clone(), false).unwrap();
    (u, d)
}

fn spec(unit: &str) -> StartSpec {
    StartSpec {
        unit: UnitId::parse(unit).unwrap(),
        image: Prepared { digest: format!("sha256:{}", "a".repeat(64)), platform: String::new() },
        argv: vec!["/home/runner/run.sh".into(), "a b".into()],
        env: BTreeMap::from([("A".to_string(), "1".to_string())]),
        credential: canary_credential(),
        sockets: None,
    }
}

#[test]
fn the_credentials_go_on_stdin_and_never_into_argv() {
    let (u, d) = driver("stdin", "sleep 30");
    let s = spec("u-1");
    u.start(&s).unwrap();
    let name = u.name(&s.unit);
    let stdin = fs::read_to_string(d.join("store").join(format!("{name}.stdin"))).unwrap();
    let argv = fs::read_to_string(d.join("store").join(format!("{name}.argv"))).unwrap();
    assert!(!argv.contains(CANARY), "{argv}");
    assert!(!stdin.contains(CANARY), "the stream is base64: {stdin}");
    for line in stdin.lines() {
        let (file, b64) = line.split_once(' ').unwrap();
        let plain = String::from_utf8(STANDARD_DECODE.decode(b64).unwrap()).unwrap();
        let want = s.credential.files.iter().find(|f| f.name == file).unwrap();
        assert_eq!(plain, want.content.expose());
    }
    assert_eq!(stdin.lines().count(), s.credential.files.len());
    let words: Vec<&str> = argv.lines().collect();
    assert_eq!(&words[..5], ["-d", "-i", "--name", name.as_str(), "--podvm-tier=incus"]);
    assert!(words.contains(&"A=1"));
    let i = words.iter().position(|w| *w == IMAGE).unwrap();
    assert_eq!(&words[i + 1..i + 3], ["/bin/sh", "-c"]);
    // The script spans lines in the file: the argv of the payload ends it.
    assert_eq!(&words[words.len() - 4..], ["podrunner", "/podrunner/.runner", "/home/runner/run.sh", "a b"]);
    assert!(u.destroy(&s.unit).unwrap().is_clean());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn an_instance_that_runs_is_ended_and_destroyed_with_no_residue() {
    let (u, d) = driver("run", "sleep 30");
    let s = spec("u-2");
    u.start(&s).unwrap();
    assert_eq!(u.start(&s).unwrap_err().code, UnitCode::UnitInUse);
    assert_eq!(u.wait(&s.unit, Duration::from_millis(200)).unwrap(), None);
    let work = u.observe(&s.unit).unwrap().unit_root.unwrap();
    assert_eq!(fs::metadata(&work).unwrap().permissions().mode() & 0o777, 0o777);
    assert_eq!(fs::metadata(work.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
    let residue = u.destroy(&s.unit).unwrap();
    assert!(residue.is_clean(), "{residue:?}");
    assert!(!u.unit_dir(&s.unit).exists());
    assert!(u.destroy(&s.unit).unwrap().is_clean(), "a second destroy counts again");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn the_exit_code_of_the_instance_is_the_exit_of_the_unit() {
    let (u, d) = driver("exit", "echo the-instance-said-42; exit 7");
    let s = spec("u-3");
    u.start(&s).unwrap();
    assert_eq!(u.wait(&s.unit, Duration::from_secs(10)).unwrap(), Some(Exit::Code(7)));
    let log = String::from_utf8(u.unit_logs(&s.unit).unwrap()).unwrap();
    assert!(log.contains("the-instance-said-42"), "{log}");
    assert!(u.destroy(&s.unit).unwrap().is_clean());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_kill_removes_the_instance_and_ends_the_unit() {
    let (u, d) = driver("kill", "sleep 30");
    let s = spec("u-4");
    u.start(&s).unwrap();
    u.signal(&s.unit, Signal::Kill).unwrap();
    assert_eq!(u.wait(&s.unit, Duration::from_secs(5)).unwrap(), Some(Exit::Signal(9)));
    assert!(u.destroy(&s.unit).unwrap().is_clean());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_replay_destroys_an_instance_that_no_driver_knows() {
    let (u, d) = driver("replay", "sleep 30");
    let s = spec("u-5");
    u.start(&s).unwrap();
    // A new driver of the same state directory: a restart of podrunner.
    let again = IncusUnit::new(u.podvm.clone(), &u.state_dir, &u.images);
    assert_eq!(again.name(&s.unit), u.name(&s.unit), "the name holds over a restart");
    assert!(again.destroy(&s.unit).unwrap().is_clean());
    assert!(u.state(&u.name(&s.unit)).is_none(), "the instance is gone");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_bad_start_leaves_nothing() {
    let (u, d) = driver("bad", "exit 0");
    let mut s = spec("u-6");
    s.image.digest = format!("sha256:{}", "b".repeat(64));
    assert_eq!(u.start(&s).unwrap_err().code, UnitCode::UnitStartFailed);
    assert!(!u.unit_dir(&s.unit).exists());
    let mut bad = spec("u-7");
    bad.credential.files[0].name = "../x".into();
    assert_eq!(u.start(&bad).unwrap_err().code, UnitCode::UnitStartFailed);
    let other = if u.platform == "linux/amd64" { "linux/arm64" } else { "linux/amd64" };
    assert_eq!(u.prepare(IMAGE, other, true).unwrap_err().code, UnitCode::ArchMismatch);
    assert!(u.prepare("runner:latest", &u.platform.clone(), false).is_err(), "a tag is refused");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn the_kind_claims_confined_and_has_no_container_route() {
    let (u, d) = driver("probe", "exit 0");
    let c = u.probe().unwrap();
    assert_eq!((c.kind.as_str(), c.boundary, c.container), ("incus", Boundary::Confined, ContainerRoute::None));
    assert_eq!(c, u.probe().unwrap(), "two probes give one record");
    let id = UnitId::parse("u-8").unwrap();
    assert_eq!(u.container_logs(&id, &ContainerId("c".into())).unwrap_err().code, UnitCode::ContainerNotSupported);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn the_name_is_one_incus_takes() {
    let n = instance_name(Path::new("/var/lib/podrunner"), "u-123456789");
    assert!(n.len() <= 63 && n.starts_with("podrunner-"), "{n}");
    assert!(n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'), "{n}");
    assert_ne!(n, instance_name(Path::new("/srv/other"), "u-123456789"), "two hosts do not meet");
}

#[test]
fn the_instance_has_no_nic_and_one_channel_to_the_egress_socket() {
    let (u, d) = driver("channel", "sleep 30");
    let mut s = spec("u-9");
    s.sockets = Some(d.join("sockets"));
    fs::create_dir_all(d.join("sockets")).unwrap();
    let _listener = std::os::unix::net::UnixListener::bind(d.join("sockets").join("egress.sock")).unwrap();
    u.start(&s).unwrap();
    let mode = fs::metadata(d.join("sockets").join("egress.sock")).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o666, "the mapped root of the instance connects to the socket");
    let argv = fs::read_to_string(d.join("store").join(format!("{}.argv", u.name(&s.unit)))).unwrap();
    let words: Vec<&str> = argv.lines().collect();
    let at = |w: &str| words.iter().position(|x| *x == w).unwrap_or_else(|| panic!("{w}: {words:?}"));
    assert_eq!(words[at("--podvm-net") + 1], "none");
    let channel = format!("{}:127.0.0.1:{EGRESS_PORT}", d.join("sockets").join("egress.sock").display());
    assert_eq!(words[at("--podvm-channel") + 1], channel);
    assert!(words.contains(&format!("{CHANNEL_PORT_VAR}={EGRESS_PORT}").as_str()), "{words:?}");
    assert!(!argv.contains("-e\nhttp_proxy"), "no proxy variable in argv: {argv}");
    assert!(u.destroy(&s.unit).unwrap().is_clean());
    let c = u.probe().unwrap();
    assert_eq!((c.net.as_str(), c.egress_hold), ("none", EgressHold::Enforced));
    let _ = fs::remove_dir_all(&d);
}
