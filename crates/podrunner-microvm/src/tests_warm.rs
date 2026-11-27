//! T-487: warm guests of the microvm kind, with a fake podvm, and the warm
//! branch of the script of the guest on the host.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use podrunner_unit::conformance::{CANARY, canary_credential};
use podrunner_unit::limits::Limits;
use podrunner_unit::warm::{ClaimSpec, WarmSpec, env_sh, wait_ready};
use podrunner_unit::{Exit, UnitCode, UnitDriver, UnitId};

use crate::tests::{dir, driver, spec};

/// A fake podvm that keeps its argv, marks the guest of `u-1` ready, waits
/// for the claim and exits 7.
const WAITS: &str = r#"[ "$1" = run ] || exit 0
d=$(dirname "$0"); printf '%s\n' "$@" > "$d/argv"
w="$d/state/units/u-1"
: > "$w/work/.podrunner-warm"
while [ ! -e "$w/cred/.podrunner-claim/go" ]; do sleep 0.05; done
exit 7"#;

#[test]
fn a_warm_guest_boots_with_no_job_and_its_claim_runs_once() {
    let (u, d) = driver("warm", WAITS);
    let u = u.with_size("4G", 4).with_pids(100);
    let s = spec("u-1");
    let env = BTreeMap::from([("PODRUNNER_UNIT".to_string(), "u-1".to_string())]);
    u.warm(&WarmSpec { unit: s.unit.clone(), image: s.image.clone(), sockets: None, env }).unwrap();
    wait_ready(&u, &s.unit, Duration::from_secs(10)).unwrap();
    let argv: Vec<String> = fs::read_to_string(d.join("argv")).unwrap().lines().map(String::from).collect();
    assert_eq!(argv.last().map(String::as_str), Some("/podrunner/.runner"), "no argv of a job: {argv:?}");
    let has = |f: &str, v: &str| argv.windows(2).any(|w| w[0] == f && w[1] == v);
    assert!(has("--podvm-cpus", "4") && has("--pids-limit", "100"), "the limits of the kind: {argv:?}");
    let unit_dir = d.join("state/units/u-1");
    assert!(has("-e", "PODRUNNER_WARM=1"), "the mark of a warm guest: {argv:?}");
    assert_eq!(fs::read_dir(unit_dir.join("cred")).unwrap().count(), 0, "no credential before the claim");
    // An offer with lower limits does not fit the guest (P2-5).
    let mut claim = ClaimSpec {
        argv: vec!["/home/runner/run.sh".into()],
        env: BTreeMap::from([("A".to_string(), "x y".to_string())]),
        credential: canary_credential(),
        limits: Limits { pids: Some(10), ..Limits::default() },
    };
    assert_eq!(u.claim(&s.unit, &claim).unwrap_err().code, UnitCode::UnitStartFailed);
    // A cold guest gets less memory for this offer (T-655).
    claim.limits = Limits { memory_bytes: Some(1 << 30), ..Limits::default() };
    assert_eq!(u.claim(&s.unit, &claim).unwrap_err().code, UnitCode::UnitStartFailed);
    claim.limits = Limits { memory_bytes: Some(64 << 30), ..Limits::default() };
    u.claim(&s.unit, &claim).unwrap();
    let f = unit_dir.join("cred/.credentials");
    assert_eq!(fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
    assert!(fs::read_to_string(&f).unwrap().contains(CANARY));
    let script = fs::read_to_string(unit_dir.join("cred/.podrunner-claim/env.sh")).unwrap();
    assert!(script.contains("export A='x y'") && script.contains("set -- '/home/runner/run.sh'"), "{script}");
    assert_eq!(u.wait(&s.unit, Duration::from_secs(10)).unwrap(), Some(Exit::Code(7)));
    assert_eq!(u.claim(&s.unit, &claim).unwrap_err().code, UnitCode::UnitInUse);
    assert_eq!(u.claim(&UnitId::parse("u-9").unwrap(), &claim).unwrap_err().code, UnitCode::UnknownUnit);
    assert!(u.destroy(&s.unit).unwrap().is_clean());
}

#[test]
fn a_unit_of_start_takes_no_claim() {
    let (u, _d) = driver("cold", "exec sleep 30");
    let s = spec("u-2");
    u.start(&s).unwrap();
    let claim =
        ClaimSpec { argv: vec![], env: BTreeMap::new(), credential: canary_credential(), limits: Limits::default() };
    assert_eq!(u.claim(&s.unit, &claim).unwrap_err().code, UnitCode::UnitInUse);
    assert!(u.destroy(&s.unit).unwrap().is_clean());
}

/// A child that a failed test does not leave waiting.
struct Ends(std::process::Child);

impl Drop for Ends {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// P2-4: the script of a warm guest, run by /bin/sh on the host with the
/// paths of the guest moved into a directory: it waits for the claim, and
/// only then starts the engine, with the variables of the job (the CA of
/// its broker); it copies each credential file at 0600 and not the claim,
/// and runs the argv of the claim.
#[test]
fn the_script_of_a_warm_guest_starts_the_engine_after_the_claim() {
    let d = dir("script");
    let p = |s: &str| d.join(s).display().to_string();
    for sub in ["cred", "work", "engine"] {
        fs::create_dir_all(d.join(sub)).unwrap();
    }
    let dockerd = format!(
        "#!/bin/sh\nenv > '{}'\nexec python3 -I -c 'import socket,sys; socket.socket(socket.AF_UNIX).bind(sys.argv[1])' '{}'\n",
        p("dockerd.env"),
        p("docker.sock")
    );
    fs::write(d.join("engine/dockerd"), dockerd).unwrap();
    fs::set_permissions(d.join("engine/dockerd"), fs::Permissions::from_mode(0o755)).unwrap();
    let script = crate::guest::SCRIPT
        .replace("/mnt/podrunner-cred", &p("cred"))
        .replace("/podrunner-work", &p("work"))
        .replace("/podrunner-engine", &p("engine"))
        .replace("/run/podrunner-docker.sock", &p("docker.sock"))
        .replace("/tmp/podrunner-dockerd.log", &p("dockerd.log"))
        .replace("id runner", "id podrunner-no-such-user-0x51");
    let mut child = Ends(
        std::process::Command::new("/bin/sh")
            .args(["-c", &script, "podrunner", &p("root")])
            .env("PODRUNNER_WARM", "1")
            .spawn()
            .unwrap(),
    );
    let end = Instant::now() + Duration::from_secs(20);
    while !d.join("work/.podrunner-warm").exists() {
        assert!(Instant::now() < end, "the guest marks no ready");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(300));
    assert!(!d.join("dockerd.env").exists(), "the engine waits for the claim");
    let env = BTreeMap::from([
        ("SSL_CERT_FILE".to_string(), "/podrunner/.runner/podrunner-ca.pem".to_string()),
        ("A".to_string(), "it's".to_string()),
    ]);
    let argv = ["sh", "-c", "echo \"$A\" > seen; exit 7"].map(String::from);
    crate::warm::write_claim(&d.join("cred"), &canary_credential(), &env_sh(&env, &argv).unwrap()).unwrap();
    let status = loop {
        if let Some(s) = child.0.try_wait().unwrap() {
            break s;
        }
        assert!(Instant::now() < end, "the guest does not end");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(7));
    let engine_env = fs::read_to_string(d.join("dockerd.env")).unwrap();
    assert!(engine_env.contains("SSL_CERT_FILE=/podrunner/.runner/podrunner-ca.pem"), "{engine_env}");
    assert!(!engine_env.contains("PODRUNNER_WARM"), "{engine_env}");
    assert_eq!(fs::read_to_string(d.join("work/seen")).unwrap(), "it's\n");
    assert!(!d.join("work/.podrunner-warm").exists());
    let f = d.join("root/.credentials");
    assert_eq!(fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
    assert!(!d.join("root/.podrunner-claim").exists() && !d.join("root/env.sh").exists());
}
