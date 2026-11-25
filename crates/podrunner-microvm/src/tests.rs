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
        mounts: Vec::new(),
        limits: Default::default(),
    }
}

#[test]
fn the_argv_holds_the_tier_the_size_and_a_read_only_volume_and_no_credential() {
    let env = BTreeMap::from([("A".to_string(), "1".to_string())]);
    let argv = vec!["/home/runner/run.sh".to_string(), "a b".to_string()];
    let shape = Shape { mem: "4G", cpus: 2, egress: None, engine: None, pids: None, init_image: false };
    let a = run_args(IMAGE, Path::new("/s/u-1"), &shape, "/home/runner", &[], &env, &argv);
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
    // A binary file is written as its bytes (T-577).
    let binary = fs::read(d.join("cred").join(podrunner_unit::conformance::BINARY_FILE)).unwrap();
    assert_eq!(binary, podrunner_unit::conformance::BINARY_BYTES);
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
    let shape = Shape {
        mem: "4G",
        cpus: 2,
        egress: Some(Path::new("/s/sockets/egress.sock")),
        engine: None,
        pids: None,
        init_image: false,
    };
    let a = run_args(IMAGE, Path::new("/s/u-1"), &shape, "/home/runner", &[], &BTreeMap::new(), &[]);
    let s = a.join(" ");
    assert!(s.contains(&format!("--podvm-net none --podvm-mem 4G --podvm-cpus 2 --report /s/u-1/run-report.json --podvm-channel /s/sockets/egress.sock:127.0.0.1:{EGRESS_PORT}")), "{s}");
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
        &Shape { mem: "4G", cpus: 2, egress: None, engine: None, pids: None, init_image: false },
        "/r",
        &[],
        &BTreeMap::new(),
        &[],
    );
    assert!(a.iter().any(|w| w.contains(MOUNT_CHECK)), "the script holds the check");
    let _ = fs::remove_dir_all(&d);
}

/// T-405: with an environment file, the argv names the file and no value.
#[test]
fn an_environment_file_keeps_the_values_out_of_the_argv() {
    let shape = Shape { mem: "4G", cpus: 2, egress: None, engine: None, pids: None, init_image: false };
    let rest = BTreeMap::from([("KEY".to_string(), "a\nb".to_string())]);
    let a =
        run_args(IMAGE, Path::new("/s/u-1"), &shape, "/r", &["--env-file".into(), "/s/u-1/env-1".into()], &rest, &[]);
    let s = a.join(" ");
    assert!(s.contains("--env-file /s/u-1/env-1"), "{s}");
    assert!(s.contains("-e KEY=a\nb"), "a value with a newline stays: {s}");
}

#[test]
fn an_engine_is_a_read_only_mount_and_the_flag_of_podvm() {
    let shape = Shape {
        mem: "4G",
        cpus: 2,
        egress: None,
        engine: Some(Path::new("/opt/docker")),
        pids: None,
        init_image: false,
    };
    let a = run_args(IMAGE, Path::new("/s/u-1"), &shape, "/r", &[], &BTreeMap::new(), &[]);
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
        &Shape { mem: "4G", cpus: 2, egress: None, engine: None, pids: None, init_image: false },
        "/r",
        &[],
        &BTreeMap::new(),
        &[],
    );
    assert!(!none.join(" ").contains("--podvm-engine"));
    let _ = fs::remove_dir_all(&d);
}

/// T-178: the payload runs as `runner` with the home of `runner`, not the
/// `HOME` of root. The tail of the script runs with stubs of `id`, `chown`,
/// `getent` and `setpriv` on `PATH`.
#[test]
fn the_payload_has_the_home_of_the_runner() {
    use std::os::unix::fs::PermissionsExt;
    let script = super::guest::SCRIPT;
    let tail = &script[script.rfind("if id runner").expect("the part of the user")..];
    let d = std::env::temp_dir().join(format!("podrunner-home-{}-{}", env!("CARGO_PKG_NAME"), std::process::id()));
    std::fs::create_dir_all(d.join("bin")).unwrap();
    for (name, body) in [
        ("id", "exit 0"),
        ("chown", "exit 0"),
        ("getent", "echo 'runner:x:1001:1001::/home/runner:/bin/sh'"),
        ("setpriv", "while [ \"$1\" != -- ]; do shift; done; shift; exec \"$@\""),
    ] {
        let f = d.join("bin").join(name);
        std::fs::write(&f, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let body = format!("root={}\n{tail}", d.display());
    let out = std::process::Command::new("sh")
        .args(["-c", &body, "sh", "sh", "-c", "echo \"$HOME $USER $LOGNAME\""])
        .env("PATH", format!("{}:/usr/bin:/bin", d.join("bin").display()))
        .env("HOME", "/root")
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&d);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "/home/runner runner runner", "{out:?}");
}

/// T-186: the run directory that the killed podvm of a guest left in the
/// store goes at `destroy`; a run of another podvm stays.
#[test]
fn destroy_removes_the_run_directory_of_the_guest() {
    let (u, d) = driver("runs", "mkdir -p \"$PODVM_STORE/machine/run-$$-1\"; exec sleep 30");
    let other = d.join("images/machine/run-1-1");
    fs::create_dir_all(&other).unwrap();
    let s = spec("u-1");
    u.start(&s).unwrap();
    let machine = d.join("images/machine");
    let t = std::time::Instant::now();
    while fs::read_dir(&machine).map(|r| r.count()).unwrap_or(0) < 2 && t.elapsed() < std::time::Duration::from_secs(5)
    {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(fs::read_dir(&machine).unwrap().count(), 2);
    u.destroy(&s.unit).unwrap();
    let left: Vec<String> =
        fs::read_dir(&machine).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    assert_eq!(left, ["run-1-1"]);
    let _ = fs::remove_dir_all(&d);
}

/// #272: with a channel, the guest script sets `NODE_USE_ENV_PROXY=1`, so
/// that the `fetch` of Node takes the proxy; a value of the job stays.
#[test]
fn the_guest_script_gives_node_the_proxy() {
    let script = crate::guest::SCRIPT;
    let from = script.find("if [ -n \"${PODRUNNER_CHANNEL_PORT").unwrap();
    let block = &script[from..from + script[from..].find("fi\n").unwrap() + 3];
    let run = |pre: &str| {
        let out = std::process::Command::new("sh")
            .args(["-c", &format!("set -eu; {pre} {block} echo \"$NODE_USE_ENV_PROXY $HTTPS_PROXY\"")])
            .env_remove("NODE_USE_ENV_PROXY")
            .env("PODRUNNER_CHANNEL_PORT", "3128")
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    assert_eq!(run(""), "1 http://127.0.0.1:3128");
    assert_eq!(run("NODE_USE_ENV_PROXY=0;"), "0 http://127.0.0.1:3128");
}

/// #274: a destroy of a unit that this process does not know (a replay of
/// the journal) removes its directory, as the podvm and incus kinds do.
#[test]
fn a_destroy_of_an_unknown_unit_removes_its_directory() {
    let d = std::env::temp_dir().join(format!("podrunner-microvm-unknown-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let u = MicrovmUnit::new(Podvm { binary: "/bin/false".into() }, &d.join("state"), &d.join("store"));
    let unit = UnitId::parse("u-7").unwrap();
    let dir = d.join("state/units/u-7");
    std::fs::create_dir_all(dir.join("work")).unwrap();
    std::fs::write(dir.join("guest.log"), "x").unwrap();
    let residue = u.destroy(&unit).unwrap();
    assert!(residue.is_clean(), "{residue:?}");
    assert!(!dir.exists(), "the directory is removed");
    let _ = std::fs::remove_dir_all(&d);
}

/// #276: the run directories of a podvm are listed while its pid is still
/// its own; a run that a later process of the same pid makes stays.
#[test]
fn the_runs_of_a_podvm_are_listed_before_its_pid_is_free() {
    let d = std::env::temp_dir().join(format!("podrunner-microvm-runs-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    let machine = d.join("machine");
    fs::create_dir_all(machine.join("run-42-a")).unwrap();
    fs::create_dir_all(machine.join("run-420-x")).unwrap();
    let runs = runs_of(&d, 42);
    assert_eq!(runs, [machine.join("run-42-a")], "run-420 is another pid");
    // The pid is free and reused: a new run of another podvm.
    fs::create_dir_all(machine.join("run-42-b")).unwrap();
    remove_runs(&runs);
    let mut left: Vec<String> =
        fs::read_dir(&machine).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    left.sort();
    assert_eq!(left, ["run-42-b", "run-420-x"]);
    let _ = fs::remove_dir_all(&d);
}

/// T-488: `pids` of the kind goes to the guest as `--pids-limit`, and the
/// record says `enforced`; with none, no flag and no claim.
#[test]
fn a_limit_of_processes_goes_to_the_guest() {
    let shape = Shape { mem: "1G", cpus: 1, egress: None, engine: None, pids: Some(64), init_image: false };
    let a = run_args(IMAGE, Path::new("/s/u-1"), &shape, "/r", &[], &BTreeMap::new(), &[]).join(" ");
    assert!(a.contains("--pids-limit 64"), "{a}");
    let none = Shape { pids: None, ..shape };
    assert!(
        !run_args(IMAGE, Path::new("/s/u-1"), &none, "/r", &[], &BTreeMap::new(), &[])
            .join(" ")
            .contains("--pids-limit")
    );
    let d = std::env::temp_dir().join(format!("podrunner-microvm-pids-{}", std::process::id()));
    let u = MicrovmUnit::new(podrunner_podvm::Podvm { binary: "/bin/true".into() }, &d, &d);
    assert_eq!(u.probe().unwrap().limit("pids"), podrunner_schema::LimitMode::None);
    let u = MicrovmUnit::new(podrunner_podvm::Podvm { binary: "/bin/true".into() }, &d, &d).with_pids(64);
    assert_eq!(u.probe().unwrap().limit("pids"), podrunner_schema::LimitMode::Enforced);
}

/// T-515: each guest writes podvm's report into the directory of its unit,
/// and the kind reads the OOM kills of the guest from it.
#[test]
fn the_oom_kills_of_a_guest_come_from_the_report_of_podvm() {
    let shape = Shape { mem: "1G", cpus: 1, egress: None, engine: None, pids: None, init_image: false };
    let a = run_args(IMAGE, Path::new("/s/u-1"), &shape, "/r", &[], &BTreeMap::new(), &[]).join(" ");
    assert!(a.contains("--report /s/u-1/run-report.json"), "{a}");
    let d = std::env::temp_dir().join(format!("podrunner-microvm-oom-{}", std::process::id()));
    let u = MicrovmUnit::new(podrunner_podvm::Podvm { binary: "/bin/true".into() }, &d, &d);
    let id = podrunner_schema::UnitId::parse("u-1").unwrap();
    let dir = d.join("units").join("u-1");
    fs::create_dir_all(&dir).unwrap();
    assert_eq!(u.oom_kills(&id), None, "no report");
    fs::write(dir.join("run-report.json"), r#"{"exit_code": 137, "oom_kills": 2}"#).unwrap();
    assert_eq!(u.oom_kills(&id), Some(2));
    let _ = fs::remove_dir_all(&d);
}

/// T-549: a kind with `init = "image"` boots the init of the image in a
/// foreground run of the machine tier (podvm refuses `run -d` there with an
/// init), and the record says `container: engine`: the image runs its own
/// `docker.service`. With no init, no flag.
#[test]
fn the_init_of_the_image_boots_in_a_foreground_guest() {
    let shape = Shape { mem: "1G", cpus: 1, egress: None, engine: None, pids: None, init_image: true };
    let a = run_args(IMAGE, Path::new("/s/u-1"), &shape, "/r", &[], &BTreeMap::new(), &[]).join(" ");
    let head = a.split(" /bin/sh ").next().unwrap();
    assert!(head.starts_with("run --rm --podvm-tier=machine") && head.contains("--podvm-init=image"), "{head}");
    assert!(!head.contains(" -d ") && !head.contains("--podvm-engine"), "{head}");
    let none = Shape { init_image: false, ..shape };
    let b = run_args(IMAGE, Path::new("/s/u-1"), &none, "/r", &[], &BTreeMap::new(), &[]).join(" ");
    assert!(!b.contains("--podvm-init"), "{b}");
    let d = std::env::temp_dir().join(format!("podrunner-microvm-init-{}", std::process::id()));
    let u = MicrovmUnit::new(podrunner_podvm::Podvm { binary: "/bin/true".into() }, &d, &d);
    assert_eq!(u.probe().unwrap().container, ContainerRoute::None);
    let u = u.with_image_init(true);
    assert_eq!(u.probe().unwrap().container, ContainerRoute::Engine);
}

/// The CPUs and the processes of a guest are the lesser of the kind and
/// the offer (T-198); the memory stays the size of the kind, because the
/// guest holds the unpacked image in it (T-573).
#[test]
fn a_guest_gets_the_lesser_cpus_and_processes_of_the_offer() {
    let (u, d) = driver("lesser", "printf '%s\\n' \"$@\" > \"$(dirname \"$0\")/argv\"");
    let u = u.with_size("4G", 4).with_pids(100);
    let argv = |unit: &str, limits: Limits| {
        let mut s = spec(unit);
        s.limits = limits;
        u.start(&s).unwrap();
        assert_eq!(u.wait(&s.unit, Duration::from_secs(10)).unwrap(), Some(Exit::Code(0)));
        assert!(u.destroy(&s.unit).unwrap().is_clean());
        fs::read_to_string(d.join("argv")).unwrap().lines().map(String::from).collect::<Vec<_>>()
    };
    let has = |a: &[String], flag: &str, v: &str| a.windows(2).any(|w| w[0] == flag && w[1] == v);
    let small = Limits { memory_bytes: Some(1 << 30), cpus: Some(2), pids: Some(50), disk_bytes: None };
    let a = argv("u-1", small);
    assert!(has(&a, "--podvm-cpus", "2") && has(&a, "--pids-limit", "50"), "{a:?}");
    assert!(has(&a, "--podvm-mem", "4G"), "the memory of the kind: {a:?}");
    let large = Limits { memory_bytes: Some(64 << 30), cpus: Some(64), pids: Some(1 << 20), disk_bytes: None };
    let a = argv("u-2", large);
    assert!(has(&a, "--podvm-cpus", "4") && has(&a, "--pids-limit", "100"), "{a:?}");
    let a = argv("u-3", Limits::default());
    assert!(has(&a, "--podvm-cpus", "4") && has(&a, "--pids-limit", "100"), "{a:?}");
    let _ = fs::remove_dir_all(&d);
}
