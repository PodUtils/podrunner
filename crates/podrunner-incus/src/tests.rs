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
/// background, with `B` the prefix of the files of the instance;
/// `inspect`, `stop`, `rm -f` and `logs` act on that state; `exec` keeps
/// its stdin (`B.claim`) and argv (`B.exec`). `inspect`
/// gives the use of `B.use` (CPU, peak, OOM kills; or a fixed one) while
/// the instance runs, as incusd. After the stop it gives `B.record`, the
/// record of the init of podvm `0.2.0-beta.39` (T-611), or `-` for each
/// field, as an older podvm. With the file `podvm.old` it refuses the
/// fields of the use, as a podvm before `0.2.0-beta.33`; with the file
/// `podvm.refuse`, `run` writes it to stderr and exits 125.
const FAKE: &str = r#"#!/bin/sh
S=$PODVM_STORE; mkdir -p "$S"
verb=$1; shift
for n; do :; done
case "$verb" in
run)
  [ -f "$0.refuse" ] && { cat "$0.refuse" >&2; exit 125; }
  name=; p=; for a; do [ "$p" = --name ] && name=$a; p=$a; done
  cat > "$S/$name.stdin"
  printf '%s\n' "$@" > "$S/$name.argv"
  echo "Running 0" > "$S/$name.status"
  export B="$S/$name"
  # The status file changes by a rename: a read never sees it empty.
  ( sh -c "$BODY" > "$S/$name.log" 2>&1; echo "Stopped $?" > "$S/$name.tmp"; mv "$S/$name.tmp" "$S/$name.status" ) </dev/null >/dev/null 2>&1 &
  echo $! > "$S/$name.pid"
  echo "$name" ;;
inspect)
  [ -f "$S/$n.status" ] || { echo "no instance $n" >&2; exit 1; }
  case "$*" in *CpuSeconds*) [ -f "$0.old" ] && { echo "no field CpuSeconds" >&2; exit 2; } ;; esac
  read -r st code < "$S/$n.status"
  # As incusd: a use while the instance runs; after, the record of podvm.
  use="- -"; [ "$st" = Running ] && { use="12.25 104857601"; [ -f "$S/$n.use" ] && use=$(cat "$S/$n.use"); }
  [ "$st" != Running ] && [ -f "$S/$n.record" ] && use=$(cat "$S/$n.record")
  case "$*" in
  *Status*CpuSeconds*) echo "$st $code $use" ;;
  *CpuSeconds*) echo "$use" ;;
  *OomKills*) set -- $use; echo "${3:--}" ;;
  *) echo "$st $code" ;;
  esac ;;
stop|rm)
  [ -f "$S/$n.pid" ] && { pkill -P "$(cat "$S/$n.pid")"; kill "$(cat "$S/$n.pid")"; } 2>/dev/null
  if [ "$verb" = rm ]; then rm -f "$S/$n".*; else echo "Stopped 143" > "$S/$n.tmp"; mv "$S/$n.tmp" "$S/$n.status"; fi ;;
logs) case "$*" in *--limit-bytes*) echo "$*" > "$S/$n.logs"; head -c "$3" "$S/$n.log" ;; *) cat "$S/$n.log" ;; esac ;;
probe) cat "$0.probe" 2>/dev/null ;;
exec)
  name=$3
  cat > "$S/$name.claim"
  printf '%s\n' "$@" > "$S/$name.exec"
  [ -f "$0.noexec" ] && { echo "the claim failed" >&2; exit 1; }
  exit 0 ;;
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
        mounts: Vec::new(),
        limits: Default::default(),
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
        let plain = STANDARD_DECODE.decode(b64).unwrap();
        let want = s.credential.files.iter().find(|f| f.name == file).unwrap();
        // A binary file goes as its bytes (T-577).
        assert_eq!(plain, podrunner_schema::FileBytes::bytes(want).unwrap().as_ref());
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

/// T-570: podvm `0.2.0-beta.37` takes `--limit-bytes`: a bounded read of
/// the log of the instance gives podvm its bound, plus one byte.
#[test]
fn a_bounded_read_of_the_log_gives_podvm_its_bound() {
    let (u, d) = driver("logs", "yes a-line | head -c 5000");
    fs::write(d.join("podvm.probe"), r#"{"podvm":"0.2.0-beta.37","rung":"namespace"}"#).unwrap();
    let s = spec("u-3");
    u.start(&s).unwrap();
    assert_eq!(u.wait(&s.unit, Duration::from_secs(10)).unwrap(), Some(Exit::Code(0)));
    let mut got = 0;
    let read = u.unit_log(&s.unit).unwrap().read_bounded(100, Duration::from_secs(5), &mut |b| got += b.len()).unwrap();
    assert_eq!((read.cut, got), (Some(podrunner_unit::Cut::Bytes), 100));
    let args = fs::read_to_string(d.join("store").join(format!("{}.logs", u.name(&s.unit)))).unwrap();
    assert_eq!(args.split_whitespace().collect::<Vec<_>>()[..3], ["--podvm-tier=incus", "--limit-bytes", "101"]);
    assert_eq!(u.unit_logs(&s.unit).unwrap().len(), 5000, "a read with no bound gets all");
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
    // #273: two state directories with one 32-bit FNV-1a hash.
    let (a, b) = (Path::new("/tmp/state-PWQ32AQYYH4h"), Path::new("/tmp/state-EFBr8apCHnCy"));
    assert_ne!(instance_name(a, "u-1"), instance_name(b, "u-1"));
}

/// #272: with a channel, the guest script sets `NODE_USE_ENV_PROXY=1`, so
/// that the `fetch` of Node takes the proxy; a value of the job stays.
#[test]
fn the_guest_script_gives_node_the_proxy() {
    let script = crate::guest::SCRIPT;
    let channel = &script[script.find("# The channel to the egress socket").unwrap()..];
    let head = &channel[..channel.find("fi\n").unwrap() + 3];
    let run = |pre: &str| {
        let out = std::process::Command::new("sh")
            .args(["-c", &format!("{pre} set -- /x; {head} echo \"$NODE_USE_ENV_PROXY $HTTPS_PROXY\"")])
            .env_remove("NODE_USE_ENV_PROXY")
            .env("PODRUNNER_CHANNEL_PORT", "3128")
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    assert_eq!(run(""), "1 http://127.0.0.1:3128");
    assert_eq!(run("NODE_USE_ENV_PROXY=0;"), "0 http://127.0.0.1:3128");
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

/// T-162: the limits of the kind go to `podvm run` as the incus keys, and
/// the record says `enforced` for each.
#[test]
fn the_limits_become_the_keys_of_the_instance() {
    use podrunner_unit::limits::Limits;
    let limits = Limits { memory_bytes: Some(2 << 30), cpus: Some(2), pids: Some(256), disk_bytes: None };
    let i = crate::guest::Instance {
        name: "n",
        reference: "r@sha256:1",
        work: std::path::Path::new("/w"),
        egress: None,
        runner_root: "/podrunner/.runner",
        limits: &limits,
        engine: None,
        payload_engine: false,
        init_image: false,
        stdin: true,
    };
    let a = run_args(&i, &[], &Default::default(), &[]).join(" ");
    assert!(a.contains("--memory 2147483648 --cpus 2 --pids-limit 256"), "{a}");
    let none =
        run_args(&crate::guest::Instance { limits: &Limits::default(), ..i }, &[], &Default::default(), &[]).join(" ");
    assert!(!none.contains("--memory") && !none.contains("--pids-limit"), "{none}");
    let (u, d) = driver("limits", "true");
    let cap = u.with_limits(limits).probe().unwrap();
    for name in ["memory", "cpu", "pids"] {
        assert_eq!(cap.limit(name), podrunner_schema::LimitMode::Enforced, "{name}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// The review of #273: a destroy of a replay also removes an instance of the
/// 32-bit name that a release before it made, so that an upgrade during a
/// job leaves no instance with the credential of the job.
#[test]
fn a_replay_destroy_removes_the_instance_of_the_old_name() {
    let (u, d) = driver("legacy", "sleep 30");
    let state = d.join("state");
    let old = crate::guest::legacy_instance_name(&state, "u-1");
    assert!(old.starts_with("podrunner-") && old.len() == "podrunner-".len() + 8 + "-u-1".len(), "{old}");
    assert_ne!(old, instance_name(&state, "u-1"));
    fs::create_dir_all(d.join("store")).unwrap();
    fs::write(d.join("store").join(format!("{old}.status")), "Running 0\n").unwrap();
    let residue = u.destroy(&UnitId::parse("u-1").unwrap()).unwrap();
    assert!(!d.join("store").join(format!("{old}.status")).exists(), "the old instance is removed");
    assert_eq!(residue.processes, Count::Known(0));
    let _ = fs::remove_dir_all(&d);
}

/// T-482: with `root_quota` the root disk of an instance gets the lesser of
/// the `disk` of the offer and of the kind (64 MiB at least), and the record
/// says `enforced` for `disk`; with none, no size and no claim.
#[test]
fn a_pool_with_quotas_gives_each_instance_a_sized_root() {
    use podrunner_unit::limits::Limits;
    fn args(l: &Limits) -> String {
        let i = crate::guest::Instance {
            name: "n",
            reference: "r@sha256:1",
            work: std::path::Path::new("/w"),
            egress: None,
            runner_root: "/r",
            limits: l,
            engine: None,
            payload_engine: false,
            init_image: false,
            stdin: true,
        };
        run_args(&i, &[], &Default::default(), &[]).join(" ")
    }
    let disk = |d: Option<u64>| Limits { disk_bytes: d, ..Limits::default() };
    let offer = Limits::of_offer(&[("disk".to_string(), 10 << 30)].into());
    let a = args(&disk(Some(40 << 30)).lesser(&offer));
    assert!(a.contains(&format!("--storage-opt size={}", 10u64 << 30)), "{a}");
    let small = args(&disk(Some(1 << 20)));
    assert!(small.contains(&format!("--storage-opt size={}", 64u64 << 20)), "{small}");
    assert!(!args(&disk(None)).contains("--storage-opt"));
    let (u, d) = driver("quota", "true");
    assert_eq!(u.probe().unwrap().limit("disk"), podrunner_schema::LimitMode::None);
    let (u2, d2) = driver("quota2", "true");
    assert_eq!(u2.with_root_quota(true).probe().unwrap().limit("disk"), podrunner_schema::LimitMode::Enforced);
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::remove_dir_all(&d2);
}

/// T-483: the CPU time and the peak memory of an instance come from the
/// state of incusd, rounded up to whole seconds; a gone instance gives
/// `unknown` with a reason.
#[test]
fn the_use_of_an_instance_comes_from_incusd() {
    let (u, d) = driver("usage", "sleep 30");
    let s = spec("u-1");
    u.start(&s).unwrap();
    assert_eq!(u.usage(&s.unit), Some(podrunner_unit::UnitUsage { cpu_s: Ok(13), peak_memory_bytes: Ok(104857601) }));
    u.destroy(&s.unit).unwrap();
    let gone = u.usage(&s.unit).unwrap();
    assert!(gone.cpu_s.as_ref().is_err_and(|e| e.contains("inspect")), "{gone:?}");
    assert!(gone.peak_memory_bytes.is_err(), "{gone:?}");
    let _ = std::fs::remove_dir_all(&d);
}

/// T-485: with an engine directory each instance gets `--podvm-engine` and
/// the directory read-only, and the record says `container: engine`.
#[test]
fn an_engine_directory_gives_each_instance_an_engine() {
    use podrunner_unit::limits::Limits;
    let i = crate::guest::Instance {
        name: "n",
        reference: "r@sha256:1",
        work: std::path::Path::new("/w"),
        egress: None,
        runner_root: "/r",
        limits: &Limits::default(),
        engine: Some(std::path::Path::new("/opt/engine")),
        payload_engine: false,
        init_image: false,
        stdin: true,
    };
    let a = run_args(&i, &[], &Default::default(), &[]).join(" ");
    assert!(a.contains("--podvm-engine -v /opt/engine:/podrunner-engine:ro"), "{a}");
    assert!(!a.contains(podrunner_unit::PAYLOAD_ENGINE_DIR));
    let bundled = crate::guest::Instance { engine: None, payload_engine: true, ..i };
    let args = run_args(&bundled, &[], &Default::default(), &[]);
    assert!(args.iter().any(|arg| arg == "--podvm-engine"));
    assert!(args.iter().any(|arg| arg.contains(&format!("{}/dockerd", podrunner_unit::PAYLOAD_ENGINE_DIR))));
    assert!(!args.windows(2).any(|v| v[0] == "-v" && v[1].contains("/podrunner-engine")));
    assert!(crate::guest::SCRIPT.contains("--bridge=none"));
    let (u, d) = driver("engine", "true");
    assert_eq!(u.probe().unwrap().container, podrunner_schema::ContainerRoute::None);
    let (u2, d2) = driver("engine2", "true");
    let cap = u2.with_engine(std::path::Path::new("/opt/engine")).probe().unwrap();
    assert_eq!(cap.container, podrunner_schema::ContainerRoute::Engine);
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::remove_dir_all(&d2);
}

/// T-543: with the init of the image each instance boots it as PID 1 with
/// the nesting of an engine, mounts no engine directory, and the record
/// says `container: engine` (the engine of the image).
#[test]
fn the_init_of_the_image_boots_with_its_own_engine() {
    use podrunner_unit::limits::Limits;
    let i = crate::guest::Instance {
        name: "n",
        reference: "r@sha256:1",
        work: std::path::Path::new("/w"),
        egress: None,
        runner_root: "/r",
        limits: &Limits::default(),
        engine: None,
        payload_engine: false,
        init_image: true,
        stdin: true,
    };
    let a = run_args(&i, &[], &Default::default(), &[]).join(" ");
    assert!(a.contains("--podvm-init=image --podvm-engine"), "{a}");
    let head = a.split(" /bin/sh ").next().unwrap();
    assert!(!head.contains("/podrunner-engine"), "{head}");
    let (u, d) = driver("init", "true");
    let cap = u.with_image_init(true).probe().unwrap();
    assert_eq!(cap.container, podrunner_schema::ContainerRoute::Engine);
    let _ = std::fs::remove_dir_all(&d);
}

mod stderr;
mod usage;
mod warm;
