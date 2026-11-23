//! The limits of an offer on an incus instance (T-162, T-198): a payload that
//! allocates past `memory` and one that forks past `pids` get an error,
//! and the same payload with no limit passes (the control). In an
//! instance the payload is root, so the keys of incus (a cgroup) hold, not
//! rlimits: the kernel ends a process past `memory`. The use of a payload
//! of a known load is the use that the kind gives (T-483). Ignored by
//! default: it needs podvm `v0.2.0-beta.33` or later and an incusd. Run:
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

/// 64 MiB in one buffer, then about 3 s of CPU in one process; then the
/// CPU time of the children of the shell (`times`, its second line).
const LOAD: &str = "dd if=/dev/zero of=/dev/null bs=64M count=1 2>/dev/null || exit 10; \
    timeout 3 sh -c 'while :; do :; done'; echo podrunner-times; times; exit 0";

/// The exit of `script` in a fresh unit with `limits`.
fn run(n: u32, script: &str, limits: Limits) -> Option<Exit> {
    run_with_usage(n, script, limits).0
}

/// The exit, the log and the use (before the destroy) of `script` in a
/// fresh unit with `limits`.
fn run_with_usage(n: u32, script: &str, limits: Limits) -> (Option<Exit>, String, Option<UnitUsage>) {
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
    let residue = unit.destroy(&id).unwrap();
    let _ = std::fs::remove_dir_all(&state);
    println!("unit {n} {limits:?}: {exit:?} {usage:?}\n{log}");
    assert!(residue.is_clean(), "{residue:?}");
    (exit, log, usage)
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
    assert_eq!(run(2, MEMORY, held), Some(Exit::Code(0)), "1 GiB must fail under 256 MiB");
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
    let (exit, log, usage) = run_with_usage(5, LOAD, Limits::default());
    assert_eq!(exit, Some(Exit::Code(0)), "the load ran: {log}");
    // The CPU time of the load, as the shell in the instance counted it.
    let line = log.lines().skip_while(|l| *l != "podrunner-times").nth(2).unwrap_or_else(|| panic!("no times: {log}"));
    let load: f64 = line.split_whitespace().map(secs).sum();
    assert!(load >= 2.0, "the load burned {load:.2} s of CPU, not about 3 s");
    let usage = usage.expect("the incus kind gives the use of its units");
    let cpu = usage.cpu_s.unwrap_or_else(|e| panic!("cpu_s unknown: {e}"));
    // The tolerance of the contract (T-520): 20 % or 1 s, the larger, of
    // the CPU time of the payload, at the whole seconds of `cpu_s`.
    let want = load.ceil();
    let tolerance = (want * 0.2).max(1.0);
    assert!((cpu as f64 - want).abs() <= tolerance, "cpu_s {cpu} for {load:.2} s of CPU of the load");
    let peak = usage.peak_memory_bytes.unwrap_or_else(|e| panic!("peak unknown: {e}"));
    assert!(peak.div_ceil(1 << 20) >= 64, "peak_rss_mib {} under the 64 MiB buffer", peak.div_ceil(1 << 20));
}
