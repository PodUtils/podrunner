use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU32, Ordering};

use podrunner_unit::conformance::{CANARY, canary_credential};

use super::*;

const IMAGE: &str = "runner@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A fresh directory of the test.
fn dir(name: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "podrunner-microvm-{name}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// A driver over a fake `podvm` that runs `body` and ignores its argv.
fn driver(name: &str, body: &str) -> (MicrovmUnit, PathBuf) {
    let d = dir(name);
    let fake = d.join("podvm");
    fs::write(&fake, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let u = MicrovmUnit::new(Podvm { binary: fake }, &d.join("state"), &d.join("images"));
    u.prepare(IMAGE, &u.platform.clone(), false).unwrap();
    (u, d)
}

fn spec(unit: &str) -> StartSpec {
    StartSpec {
        unit: UnitId::parse(unit).unwrap(),
        image: Prepared { digest: format!("sha256:{}", "a".repeat(64)), platform: String::new() },
        argv: vec!["/home/runner/run.sh".into()],
        env: BTreeMap::new(),
        credential: canary_credential(),
        sockets: None,
    }
}

#[test]
fn the_argv_holds_the_tier_the_size_and_a_read_only_volume_and_no_credential() {
    let env = BTreeMap::from([("A".to_string(), "1".to_string())]);
    let argv = vec!["/home/runner/run.sh".to_string(), "a b".to_string()];
    let shape = Shape { mem: "4G", cpus: 2, egress: None, engine: None };
    let a = run_args(IMAGE, Path::new("/s/u-1"), &shape, "/home/runner", &env, &argv);
    let s = a.join(" ");
    for want in ["run --rm --podvm-tier=machine --podvm-net none", "--podvm-mem 4G", "--podvm-cpus 2", "-e A=1"] {
        assert!(s.contains(want), "{want}: {s}");
    }
    assert!(
        !s.contains("--podvm-channel") && !s.contains(&format!("{CHANNEL_PORT_VAR}=")),
        "no egress socket, no way out: {s}"
    );
    assert!(a.contains(&format!("/s/u-1/cred:{CRED_MOUNT}:ro")));
    assert!(a.contains(&format!("/s/u-1/work:{WORK_MOUNT}")));
    let i = a.iter().position(|w| w == IMAGE).unwrap();
    assert_eq!(&a[i + 1..i + 3], ["/bin/sh", "-c"]);
    assert_eq!(&a[i + 4..], ["podrunner", "/home/runner", "/home/runner/run.sh", "a b"], "argv as positional words");
    assert!(!s.contains(CANARY));
}

#[test]
fn the_credential_files_are_private_and_a_name_that_is_not_plain_is_refused() {
    let d = dir("cred");
    guest::write_credentials(&d.join("cred"), &canary_credential()).unwrap();
    assert_eq!(fs::metadata(d.join("cred")).unwrap().permissions().mode() & 0o777, 0o700);
    let f = d.join("cred/.runner");
    assert_eq!(fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
    assert!(fs::read_to_string(f).unwrap().contains(CANARY));
    let mut bad = canary_credential();
    bad.files[0].name = "../x".into();
    assert_eq!(guest::write_credentials(&d.join("bad"), &bad).unwrap_err().code, UnitCode::UnitStartFailed);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_guest_that_runs_is_ended_and_destroyed_with_no_residue() {
    let (u, d) = driver("run", "exec sleep 30");
    let s = spec("u-1");
    u.start(&s).unwrap();
    assert_eq!(u.start(&s).unwrap_err().code, UnitCode::UnitInUse);
    assert_eq!(u.wait(&s.unit, Duration::from_millis(200)).unwrap(), None);
    let seen = u.observe(&s.unit).unwrap();
    assert!(!seen.pids.is_empty(), "the tag reaches the process of podvm");
    assert!(seen.runner_root.unwrap().join(".credentials").exists());
    let work = seen.unit_root.unwrap();
    assert_eq!(fs::metadata(&work).unwrap().permissions().mode() & 0o777, 0o777);
    assert_eq!(fs::metadata(work.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
    let residue = u.destroy(&s.unit).unwrap();
    assert!(residue.is_clean(), "{residue:?}");
    assert!(!u.unit_dir(&s.unit).exists());
    assert!(u.destroy(&s.unit).unwrap().is_clean(), "a second destroy counts again");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn the_exit_code_of_the_guest_is_the_exit_of_the_unit() {
    let (u, d) = driver("exit", "echo the-guest-said-42; exit 7");
    let s = spec("u-2");
    u.start(&s).unwrap();
    assert_eq!(u.wait(&s.unit, Duration::from_secs(10)).unwrap(), Some(Exit::Code(7)));
    let log = String::from_utf8(u.unit_logs(&s.unit).unwrap()).unwrap();
    assert!(log.contains("the-guest-said-42"), "{log}");
    assert!(u.destroy(&s.unit).unwrap().is_clean());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn an_image_that_is_not_prepared_or_of_another_platform_is_refused() {
    let (u, d) = driver("refuse", "exit 0");
    let mut s = spec("u-3");
    s.image.digest = format!("sha256:{}", "b".repeat(64));
    assert_eq!(u.start(&s).unwrap_err().code, UnitCode::UnitStartFailed);
    assert!(!u.unit_dir(&s.unit).exists());
    let other = if u.platform == "linux/amd64" { "linux/arm64" } else { "linux/amd64" };
    assert_eq!(u.prepare(IMAGE, other, true).unwrap_err().code, UnitCode::ArchMismatch);
    assert!(u.prepare("runner:latest", &u.platform.clone(), false).is_err(), "a tag is refused");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn the_kind_claims_a_guest_and_has_no_container_route() {
    let (u, d) = driver("probe", "exit 0");
    let c = u.probe().unwrap();
    assert_eq!((c.kind.as_str(), c.boundary, c.container), ("microvm", Boundary::Guest, ContainerRoute::None));
    assert_eq!(c, u.probe().unwrap(), "two probes give one record");
    let id = UnitId::parse("u-4").unwrap();
    let e = u.container_logs(&id, &ContainerId("c".into())).unwrap_err();
    assert_eq!(e.code, UnitCode::ContainerNotSupported);
    let engine = u.with_engine(&d);
    assert_eq!(engine.probe().unwrap().container, ContainerRoute::Engine, "an engine in each guest");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn an_egress_socket_is_the_one_channel_and_the_proxy_of_the_payload() {
    let shape = Shape { mem: "4G", cpus: 2, egress: Some(Path::new("/s/sockets/egress.sock")), engine: None };
    let a = run_args(IMAGE, Path::new("/s/u-1"), &shape, "/home/runner", &BTreeMap::new(), &[]);
    let s = a.join(" ");
    assert!(s.contains(&format!("--podvm-net none --podvm-mem 4G --podvm-cpus 2 --podvm-channel /s/sockets/egress.sock:127.0.0.1:{EGRESS_PORT}")), "{s}");
    assert!(s.contains(&format!("-e {CHANNEL_PORT_VAR}={EGRESS_PORT}")), "{s}");
    assert!(
        !s.contains("-e http_proxy") && !s.contains("-e https_proxy"),
        "no proxy variable that podvm rewrites: {s}"
    );
    assert_eq!(s.matches("--podvm-channel").count(), 1);
}

#[test]
fn the_mount_check_takes_only_the_table_of_the_unit() {
    let d = dir("mounts");
    let base = "rootfs / rw 0 0\nproc /proc rw 0 0\n";
    let cred = "podrunner-cred /mnt/podrunner-cred 9p ro,relatime,trans=virtio 0 0\n";
    let work = "podrunner-work /podrunner-work 9p rw,relatime,trans=virtio 0 0\n";
    let check = |table: &str| {
        let f = d.join("mounts");
        fs::write(&f, table).unwrap();
        std::process::Command::new("awk").arg(MOUNT_CHECK).arg(&f).status().unwrap().success()
    };
    assert!(check(&format!("{base}{cred}{work}")), "the table of the unit");
    assert!(check(base), "no host mount");
    assert!(!check(&format!("{base}{}{work}", cred.replace(" ro,", " rw,"))), "a writable credential directory");
    assert!(!check(&format!("{base}{cred}{work}home /mnt/home 9p rw 0 0\n")), "a host path outside the table");
    assert!(!check(&format!("{base}{cred}{work}fs /data virtiofs rw 0 0\n")), "a virtiofs mount");
    let a = run_args(
        IMAGE,
        Path::new("/s/u-1"),
        &Shape { mem: "4G", cpus: 2, egress: None, engine: None },
        "/r",
        &BTreeMap::new(),
        &[],
    );
    assert!(a.iter().any(|w| w.contains(MOUNT_CHECK)), "the script holds the check");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn an_engine_is_a_read_only_mount_and_the_flag_of_podvm() {
    let shape = Shape { mem: "4G", cpus: 2, egress: None, engine: Some(Path::new("/opt/docker")) };
    let a = run_args(IMAGE, Path::new("/s/u-1"), &shape, "/r", &BTreeMap::new(), &[]);
    let s = a.join(" ");
    assert!(s.contains("--podvm-engine"), "{s}");
    assert!(a.contains(&format!("/opt/docker:{ENGINE_MOUNT}:ro")), "{s}");
    assert!(s.contains("dockerd --host=unix:///run/podrunner-docker.sock"), "the script starts the engine");
    let d = dir("engine-mounts");
    let f = d.join("mounts");
    let ok = |table: &str| {
        fs::write(&f, table).unwrap();
        std::process::Command::new("awk").arg(MOUNT_CHECK).arg(&f).status().unwrap().success()
    };
    assert!(ok("e /podrunner-engine 9p ro,trans=virtio 0 0\n"), "a read-only engine directory");
    assert!(!ok("e /podrunner-engine 9p rw,trans=virtio 0 0\n"), "a writable engine directory");
    let none = run_args(
        IMAGE,
        Path::new("/s/u-1"),
        &Shape { mem: "4G", cpus: 2, egress: None, engine: None },
        "/r",
        &BTreeMap::new(),
        &[],
    );
    assert!(!none.join(" ").contains("--podvm-engine"));
    let _ = fs::remove_dir_all(&d);
}
