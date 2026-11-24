//! The limits of an offer on an incus instance (T-162, T-198): a payload that
//! allocates past `memory` and one that forks past `pids` get an error,
//! and the same payload with no limit passes (the control). In an
//! instance the payload is root, so the keys of incus (a cgroup) hold, not
//! rlimits: the kernel ends a process past `memory`, and the kind counts
//! the OOM kill. The use of a payload of a known load is the use that the
//! kind gives (T-483, T-611). Ignored by default: it needs podvm
//! `v0.2.0-beta.39` or later (the record of the use at the stop) and an
//! incusd. Run:
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_PODVM_IMAGE=NAME@sha256:HEX \
//!   cargo test -p podrunner-incus --test limits -- --ignored --nocapture
//!
//! The image needs sh, dd, sleep and timeout.

use std::path::{Path, PathBuf};
use std::time::Duration;

use podrunner_incus::IncusUnit;
use podrunner_podvm::Podvm;
use podrunner_schema::UnitId;
use podrunner_unit::limits::Limits;
use podrunner_unit::{Exit, StartSpec, UnitDriver, UnitUsage};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// A small buffer passes (exit 10 if not); a buffer of 1 GiB fails (exit
/// 11 if not); then exit 0.
const MEMORY: &str = "dd if=/dev/zero of=/dev/null bs=1M count=1 2>/dev/null || exit 10; \
    if dd if=/dev/zero of=/dev/null bs=1024M count=1 2>/dev/null; then exit 11; fi; exit 0";
/// 1000 processes at one time: exit 12 when each fork passed.
const FORKS: &str = "i=0; while [ $i -lt 1000 ]; do sleep 30 & i=$((i+1)); done; exit 12";

/// 64 MiB in one buffer, then about 3 s of CPU in each of 4 processes at
/// one time (T-611: the last tick of 4 busy CPUs); then the CPU time of
/// the children of the shell (`times`, its second line).
const LOAD: &str = "dd if=/dev/zero of=/dev/null bs=64M count=1 2>/dev/null || exit 10; \
    for i in 1 2 3 4; do timeout 3 sh -c 'while :; do :; done' & done; wait; \
    echo podrunner-times; times; exit 0";

/// The exit of `script` in a fresh unit with `limits`.
fn run(n: u32, script: &str, limits: Limits) -> Option<Exit> {
    run_with_usage(n, script, limits).exit
}

/// The reason of an unknown peak and of no OOM kill that the tests take,
/// by name: podvm `0.2.0-beta.39` gave no record of the use at the stop
/// on a hosted runner (runner-image.yml 45359834173, COORD-128 #350,
/// T-611). The record of the stopped instance (its raw `inspect`) must
/// show it: `-` for its peak.
const PEAK_NOT_GIVEN: &str = "incusd gave no PeakMemoryBytes of the instance";
const NO_RECORD: &str = "podvm 0.2.0-beta.39 gave no record of the use at the stop (COORD-128 #350, T-611)";

/// What a unit gave: its exit, its log, its use and OOM kills (before the
/// destroy), and the raw `inspect` of the stopped instance.
struct Ran {
    exit: Option<Exit>,
    log: String,
    usage: Option<UnitUsage>,
    oom: Option<u64>,
    raw: String,
}

impl Ran {
    /// The stopped instance has no record of the use: its peak is `-`.
    fn no_record(&self) -> bool {
        self.raw.split_whitespace().nth(3) == Some("-")
    }
}

/// The raw state of the instance NAME, for the evidence of COORD-128: the
/// template of the kind (`usage::STATE`).
fn raw_inspect(name: &str) -> String {
    let format = "{{.Status}} {{.ExitCode}} {{.CpuSeconds}} {{.PeakMemoryBytes}} {{.OomKills}}";
    let out = std::process::Command::new(env("PODRUNNER_PODVM"))
        .args(["inspect", "--format", format, "--podvm-tier=incus", name])
        .env("PODVM_STORE", env("PODRUNNER_PODVM_IMAGES"))
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let text =
        format!("{}{}", String::from_utf8_lossy(&out.stdout).trim(), String::from_utf8_lossy(&out.stderr).trim());
    println!("raw: podvm inspect --format '{format}' --podvm-tier=incus {name}: {:?}: {text}", out.status.code());
    text
}

/// The exit, the log, the use and the OOM kills (before the destroy) of
/// `script` in a fresh unit with `limits`.
fn run_with_usage(n: u32, script: &str, limits: Limits) -> Ran {
    let state = std::env::temp_dir().join(format!("podrunner-incus-limits-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let unit = IncusUnit::new(
        Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) },
        &state,
        Path::new(&env("PODRUNNER_PODVM_IMAGES")),
    );
    let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let image = unit.prepare(&env("PODRUNNER_PODVM_IMAGE"), platform, false).unwrap();
    let id = UnitId::parse(&format!("u-{n}")).unwrap();
    let spec = StartSpec {
        unit: id.clone(),
        image,
        argv: ["sh", "-c", script].map(String::from).to_vec(),
        env: Default::default(),
        credential: Default::default(),
        sockets: None,
        mounts: Vec::new(),
        limits,
    };
    unit.start(&spec).unwrap();
    let exit = unit.wait(&id, Duration::from_secs(120)).unwrap();
    let log = String::from_utf8_lossy(&unit.unit_logs(&id).unwrap_or_default()).to_string();
    let usage = unit.usage(&id);
    let oom = unit.oom_kills(&id);
    let raw = raw_inspect(&podrunner_incus::instance_name(&state, id.as_str()));
    let residue = unit.destroy(&id).unwrap();
    let _ = std::fs::remove_dir_all(&state);
    println!("unit {n} {limits:?}: {exit:?} {usage:?} oom_kills {oom:?}\n{log}");
    assert!(residue.is_clean(), "{residue:?}");
    Ran { exit, log, usage, oom, raw }
}

/// The seconds of one word of `times` (`0m2.980s` of dash or bash).
fn secs(word: &str) -> f64 {
    let (m, s) = word.trim_end_matches('s').split_once('m').unwrap_or_else(|| panic!("not a time: {word}"));
    m.parse::<f64>().unwrap() * 60.0 + s.parse::<f64>().unwrap()
}

#[test]
#[ignore]
fn a_payload_past_the_memory_limit_gets_an_error() {
    assert_eq!(run(1, MEMORY, Limits::default()), Some(Exit::Code(11)), "the control: 1 GiB passes with no limit");
    let held = Limits { memory_bytes: Some(256 << 20), ..Limits::default() };
    let ran = run_with_usage(2, MEMORY, held);
    assert_eq!(ran.exit, Some(Exit::Code(0)), "1 GiB must fail under 256 MiB");
    // The exit reason `limit` of slots needs the count (T-515, T-611).
    match ran.oom {
        Some(n) if n > 0 => {}
        Some(0) if ran.no_record() => println!("oom_kills 0 under 256 MiB: {NO_RECORD}; raw: {}", ran.raw),
        oom => panic!("no OOM kill counted under 256 MiB: {oom:?}; raw: {}", ran.raw),
    }
}

#[test]
#[ignore]
fn a_payload_past_the_pids_limit_gets_an_error() {
    assert_eq!(run(3, FORKS, Limits::default()), Some(Exit::Code(12)), "the control: 1000 forks pass with no limit");
    let held = Limits { pids: Some(256), ..Limits::default() };
    let exit = run(4, FORKS, held);
    assert!(matches!(exit, Some(Exit::Code(c)) if c != 0 && c != 12), "1000 forks must fail under pids 256: {exit:?}");
}

#[test]
#[ignore]
fn the_use_of_a_known_load_is_the_use_that_the_kind_gives() {
    let ran = run_with_usage(5, LOAD, Limits::default());
    let (exit, log, usage) = (ran.exit, &ran.log, ran.usage.clone());
    assert_eq!(exit, Some(Exit::Code(0)), "the load ran: {log}");
    // The CPU time of the load, as the shell in the instance counted it.
    let line = log.lines().skip_while(|l| *l != "podrunner-times").nth(2).unwrap_or_else(|| panic!("no times: {log}"));
    let load: f64 = line.split_whitespace().map(secs).sum();
    assert!(load >= 2.0, "the load burned {load:.2} s of CPU, not about 3 s on each CPU");
    let usage = usage.expect("the incus kind gives the use of its units");
    let cpu = usage.cpu_s.unwrap_or_else(|e| panic!("cpu_s unknown: {e}"));
    // The tolerance of the contract (T-520): 20 % or 1 s, the larger, of
    // the CPU time of the payload, at the whole seconds of `cpu_s`.
    let want = load.ceil();
    let tolerance = (want * 0.2).max(1.0);
    assert!((cpu as f64 - want).abs() <= tolerance, "cpu_s {cpu} for {load:.2} s of CPU of the load");
    // The incusd of a hosted runner gives no peak of a running instance
    // on cgroup v2; the record of podvm at the stop gives it (T-611).
    match usage.peak_memory_bytes {
        Ok(peak) => {
            assert!(peak.div_ceil(1 << 20) >= 64, "peak_rss_mib {} under the 64 MiB buffer", peak.div_ceil(1 << 20))
        }
        Err(why) if why == PEAK_NOT_GIVEN && ran.no_record() => {
            println!("peak_memory_bytes: unknown ({why}): {NO_RECORD}; raw: {}", ran.raw)
        }
        Err(why) => panic!("peak unknown: {why}; raw: {}", ran.raw),
    }
}
