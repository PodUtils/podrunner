//! The limits of an offer on a podvm unit (T-162, T-198): a payload that
//! allocates past `memory` and one that forks past `pids` get an error,
//! and the same payload with no limit passes (the control). Ignored by
//! default. Run:
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_PODVM_IMAGE=NAME@sha256:HEX \
//!   cargo test -p podrunner-podvm --test limits -- --ignored --nocapture
//!
//! The image needs sh, dd and sleep.

use std::path::{Path, PathBuf};
use std::time::Duration;

mod scratch;

use podrunner_podvm::{Podvm, PodvmUnit};
use podrunner_schema::UnitId;
use podrunner_unit::limits::Limits;
use podrunner_unit::{Exit, StartSpec, UnitDriver};
use scratch::Scratch;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// A small buffer passes (exit 10 if not); a buffer of 1 GiB fails (exit
/// 11 if not); then exit 0.
const MEMORY: &str = "dd if=/dev/zero of=/dev/null bs=1M count=1 2>/dev/null || exit 10; \
    if dd if=/dev/zero of=/dev/null bs=1024M count=1 2>/dev/null; then exit 11; fi; exit 0";
/// 1000 processes at one time: exit 12 when each fork passed.
const FORKS: &str = "i=0; while [ $i -lt 1000 ]; do sleep 30 & i=$((i+1)); done; exit 12";

/// The exit of `script` in a fresh unit with `limits`.
fn run(n: u32, script: &str, limits: Limits) -> Option<Exit> {
    run_oom(n, script, limits).0
}

/// The exit of `script` in a fresh unit with `limits`, and its OOM kills
/// (before the destroy).
fn run_oom(n: u32, script: &str, limits: Limits) -> (Option<Exit>, Option<u64>) {
    let state = Scratch::new(&format!("podvm-limits-{n}"));
    let unit = PodvmUnit::new(
        Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) },
        &state,
        Path::new(&env("PODRUNNER_PODVM_IMAGES")),
    )
    .unwrap();
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
    let oom = unit.oom_kills(&id);
    let report = unit.run_report(&id);
    let residue = unit.destroy(&id).unwrap();
    println!("unit {n} {limits:?}: {exit:?} oom_kills {oom:?}\n{log}\nthe run report: {report:?}");
    assert!(residue.is_clean(), "{residue:?}");
    (exit, oom)
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

/// The `cpu.max` of each cgroup of a podvm run beside `own` (the cgroup of
/// this test), read on the host: a unit has no cgroup file system in its
/// root, so its payload cannot read its own quota (runner-image
/// 44153617910: exit 13).
fn cpu_max_beside(own: &Path) -> Vec<(PathBuf, String)> {
    file_beside(own, "cpu.max")
}

/// The file `name` of each cgroup `podvm-*` beside `own`.
fn file_beside(own: &Path, name: &str) -> Vec<(PathBuf, String)> {
    let Some(parent) = own.parent() else { return Vec::new() };
    std::fs::read_dir(parent)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("podvm-"))
        .filter_map(|e| std::fs::read_to_string(e.path().join(name)).ok().map(|q| (e.path(), q.trim().to_string())))
        .collect()
}

/// T-221: where the host delegates a cgroup v2 subtree with the `cpu`
/// controller to the user, a unit holds `cpus` of its limits: the cgroup of
/// its podvm run has the quota of one CPU. Run it in such a subtree
/// (`systemd-run --uid=USER -p Delegate=yes`).
#[test]
#[ignore]
fn a_cpu_limit_holds_in_a_delegated_cgroup() {
    // Only the measure of `runner-image.yml` runs in a delegated cgroup.
    if std::env::var_os("PODRUNNER_CPU_DELEGATED").is_none() {
        println!("skip: PODRUNNER_CPU_DELEGATED is not set");
        return;
    }
    let cg = std::fs::read_to_string("/proc/self/cgroup").unwrap();
    let own = PathBuf::from(format!("/sys/fs/cgroup{}", cg.trim().trim_start_matches("0::")));
    let state = Scratch::new("podvm-limits-5");
    let unit = PodvmUnit::new(
        Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) },
        &state,
        Path::new(&env("PODRUNNER_PODVM_IMAGES")),
    )
    .unwrap();
    let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let image = unit.prepare(&env("PODRUNNER_PODVM_IMAGE"), platform, false).unwrap();
    let id = UnitId::parse("u-5").unwrap();
    let spec = StartSpec {
        unit: id.clone(),
        image,
        argv: ["sh", "-c", "sleep 30"].map(String::from).to_vec(),
        env: Default::default(),
        credential: Default::default(),
        sockets: None,
        mounts: Vec::new(),
        limits: Limits { cpus: Some(1), ..Limits::default() },
    };
    let before = cpu_max_beside(&own);
    unit.start(&spec).unwrap();
    let end = std::time::Instant::now() + Duration::from_secs(20);
    let mut seen = Vec::new();
    while std::time::Instant::now() < end {
        seen = cpu_max_beside(&own).into_iter().filter(|c| !before.contains(c)).collect();
        // podvm can make the cgroup before it writes the quota: wait for it.
        if seen.iter().any(|(_, q)| q == "100000 100000") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let residue = unit.destroy(&id).unwrap();
    println!("own cgroup {}; the cgroups of the run: {seen:?}", own.display());
    assert!(residue.is_clean(), "{residue:?}");
    assert!(!seen.is_empty(), "no cgroup podvm-* of the run beside {}", own.display());
    assert!(seen.iter().any(|(_, q)| q == "100000 100000"), "cpu.max of the run must be one CPU: {seen:?}");
}

/// T-521: where the host delegates a cgroup v2 subtree with the `pids`
/// controller to the user, two units of that user each hold their own
/// `pids` in the leaf of their run (`pids.max`), not one RLIMIT_NPROC of
/// the user. The same delegated subtree as the CPU limit.
#[test]
#[ignore]
fn each_unit_holds_its_own_pids_in_a_delegated_cgroup() {
    if std::env::var_os("PODRUNNER_CPU_DELEGATED").is_none() {
        println!("skip: PODRUNNER_CPU_DELEGATED is not set");
        return;
    }
    let cg = std::fs::read_to_string("/proc/self/cgroup").unwrap();
    let own = PathBuf::from(format!("/sys/fs/cgroup{}", cg.trim().trim_start_matches("0::")));
    let state = Scratch::new("podvm-limits-6");
    let unit = PodvmUnit::new(
        Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) },
        &state,
        Path::new(&env("PODRUNNER_PODVM_IMAGES")),
    )
    .unwrap();
    let mode = unit.probe().unwrap().limit(podrunner_podvm::PIDS_PER_UNIT);
    println!("own cgroup {}; {}: {mode:?}", own.display(), podrunner_podvm::PIDS_PER_UNIT);
    let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let image = unit.prepare(&env("PODRUNNER_PODVM_IMAGE"), platform, false).unwrap();
    let before = file_beside(&own, "pids.max");
    let mut ids = Vec::new();
    for (n, pids) in [(6, 64), (7, 96)] {
        let id = UnitId::parse(&format!("u-{n}")).unwrap();
        let spec = StartSpec {
            unit: id.clone(),
            image: image.clone(),
            argv: ["sh", "-c", "sleep 30"].map(String::from).to_vec(),
            env: Default::default(),
            credential: Default::default(),
            sockets: None,
            mounts: Vec::new(),
            limits: Limits { pids: Some(pids), ..Limits::default() },
        };
        unit.start(&spec).unwrap();
        ids.push(id);
    }
    let end = std::time::Instant::now() + Duration::from_secs(20);
    let mut seen: Vec<String> = Vec::new();
    while std::time::Instant::now() < end {
        let new = file_beside(&own, "pids.max").into_iter().filter(|c| !before.contains(c));
        seen = new.map(|(_, v)| v).collect();
        // podvm can make the cgroup before it writes the limit: wait for it.
        if seen.contains(&"64".to_string()) && seen.contains(&"96".to_string()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    for id in &ids {
        let residue = unit.destroy(id).unwrap();
        assert!(residue.is_clean(), "{residue:?}");
    }
    println!("pids.max of the runs: {seen:?}");
    assert_eq!(mode, podrunner_schema::LimitMode::Enforced, "the record must say pids for each unit");
    assert!(
        seen.contains(&"64".to_string()) && seen.contains(&"96".to_string()),
        "each run, its own pids.max: {seen:?}"
    );
}

/// T-515: where the host delegates a cgroup v2 subtree with the `memory`
/// controller to the user, the memory of a unit is `memory.max` of the
/// leaf of its run (`--memory`), not RLIMIT_AS: the kernel ends the
/// process past it, and the run report of podvm counts the kill, which
/// slots turns into the exit reason `limit`. The same delegated subtree as
/// the CPU limit.
#[test]
#[ignore]
fn a_unit_past_its_memory_counts_an_oom_kill_in_a_delegated_cgroup() {
    if std::env::var_os("PODRUNNER_CPU_DELEGATED").is_none() {
        println!("skip: PODRUNNER_CPU_DELEGATED is not set");
        return;
    }
    let state = Scratch::new("podvm-limits-probe");
    let unit = PodvmUnit::new(
        Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) },
        &state,
        Path::new(&env("PODRUNNER_PODVM_IMAGES")),
    )
    .unwrap();
    let mode = unit.probe().unwrap().limit(podrunner_podvm::MEMORY_PER_UNIT);
    println!("{}: {mode:?}", podrunner_podvm::MEMORY_PER_UNIT);
    assert_eq!(mode, podrunner_schema::LimitMode::Enforced, "the record must say memory for each unit");
    let held = Limits { memory_bytes: Some(256 << 20), ..Limits::default() };
    let (exit, oom) = run_oom(8, MEMORY, held);
    assert_eq!(exit, Some(Exit::Code(0)), "1 GiB must fail under 256 MiB");
    assert!(oom.is_some_and(|n| n > 0), "no OOM kill counted under 256 MiB: {oom:?}");
}
