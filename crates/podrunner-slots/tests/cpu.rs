//! `cpu_s` of a known load (T-520, githome's G-06). A payload burns a
//! known CPU time in one process, and the sampler of slots
//! (`podrunner_slots::usage::Usage`) measures the unit as `serve` does: one
//! sample of the processes of the unit (`observe`) at each tick of 1 s,
//! then the same `cpu_s` as the report. The payload counts its own CPU
//! time (`times`). `cpu_s` must be within the tolerance of
//! `docs/contracts/source-driver.md`: 20 % or 1 s of the CPU time of the
//! payload, the larger. On the microvm kind the process on the host is the
//! emulator: its CPU time also counts the boot and the stop of the guest,
//! so the tolerance holds for `cpu_s` less the `cpu_s` of an idle guest
//! (the control).
//!
//! Ignored by default. `PODRUNNER_CPU_KIND` names the kind (`fake`,
//! `podvm` or `microvm`); with no kind the test does nothing. Run:
//!
//!   PODRUNNER_CPU_KIND=fake cargo test -p podrunner-slots --test cpu -- --ignored --nocapture
//!
//!   PODRUNNER_CPU_KIND=podvm PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_PODVM_IMAGE=NAME@sha256:HEX \
//!   cargo test -p podrunner-slots --test cpu -- --ignored --nocapture
//!
//! The image needs sh, sleep and timeout.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use podrunner_schema::{Residual, UnitId};
use podrunner_slots::usage::Usage;
use podrunner_unit::{Exit, StartSpec, UnitDriver};

/// The tick of `serve`: the sampler reads the unit once for each tick.
const TICK: Duration = Duration::from_secs(1);
/// The line before the output of `times`.
const MARK: &str = "podrunner-times";
/// About 8 s of CPU in one process. The trap keeps the process for 2 s
/// (more than one tick) after the burn, so that a sample sees its last CPU
/// time: the loss of the last tick is a separate clause of the contract.
const LOAD: &str = "timeout 8 sh -c 'trap \"sleep 2; exit 0\" TERM; while :; do :; done'; \
    echo podrunner-times; times";
/// No load: the control. It holds for 2 s, so that a sample sees the unit
/// (a unit that no sample saw has an `unknown` `cpu_s`).
const IDLE: &str = "sleep 2; echo podrunner-times; times";

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// The tolerance of `cpu_s` against `load` seconds of CPU of the payload:
/// 20 % or 1 s, the larger, of the measure before its round up. `cpu_s`
/// is the next whole second of a measure in (`cpu_s` - 1, `cpu_s`], so it
/// holds when such a measure is within the tolerance of `load`.
fn within(cpu_s: u64, load: f64) -> bool {
    podrunner_slots::usage::cpu_s_within(cpu_s, load)
}

/// The seconds of one word of `times` (`0m2.980s` of dash or bash).
fn secs(word: &str) -> Option<f64> {
    let (m, s) = word.strip_suffix('s')?.split_once('m')?;
    Some(m.parse::<f64>().ok()? * 60.0 + s.parse::<f64>().ok()?)
}

/// The CPU time of the children of the payload shell: the second line of
/// `times` after the mark. A guest console can add `\r` and other lines.
fn load_of(log: &str) -> Option<f64> {
    let lines: Vec<&str> = log.lines().map(str::trim).collect();
    let at = lines.iter().rposition(|l| *l == MARK)?;
    let mut times = lines[at + 1..].iter().filter_map(|l| {
        let words: Vec<&str> = l.split_whitespace().collect();
        let v: Option<Vec<f64>> = words.iter().map(|w| secs(w)).collect();
        v.filter(|v| v.len() == 2).map(|v| v.iter().sum::<f64>())
    });
    times.next()?;
    times.next()
}

/// A directory of the test that goes with it, also on a panic.
struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The driver of the kind `kind` with its state in `dir`, and the image of
/// the payload.
fn driver(kind: &str, dir: &Path) -> (Box<dyn UnitDriver>, String) {
    let podvm = || podrunner_podvm::Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) };
    let images = || PathBuf::from(env("PODRUNNER_PODVM_IMAGES"));
    match kind {
        "fake" => {
            let cap = serde_json::from_value(serde_json::json!({
                "schema": 1, "kind": "fake", "arch": "x86_64", "boundary": "confined", "principal": "distinct",
                "container": "hooks", "net": "broker", "secret_delivery": ["files"],
                "credential_store": "private-dir", "mount_modes": ["rw"], "rootfs_fresh": "yes",
                "tree_kill": "verified", "limits": {}, "egress_hold": "advisory", "kvm": "no"
            }))
            .unwrap();
            let image = format!("busybox@sha256:{}", "0".repeat(64));
            (Box::new(podrunner_fake::FakeUnit::new(dir, cap, platform())), image)
        }
        "podvm" => {
            let unit = podrunner_podvm::PodvmUnit::new(podvm(), dir, &images()).unwrap();
            (Box::new(unit), env("PODRUNNER_PODVM_IMAGE"))
        }
        "microvm" => {
            let unit = podrunner_microvm::MicrovmUnit::new(podvm(), dir, &images());
            (Box::new(unit), env("PODRUNNER_PODVM_IMAGE"))
        }
        other => panic!("PODRUNNER_CPU_KIND {other}: not fake, podvm or microvm"),
    }
}

fn platform() -> &'static str {
    if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" }
}

/// What a unit gave: the CPU time that the payload counted, and `cpu_s`.
struct Measured {
    load: f64,
    cpu_s: u64,
}

/// Run `script` in a fresh unit of `kind` and sample it at each tick until
/// it ends, as `serve` does.
fn measure(kind: &str, n: u32, script: &str) -> Measured {
    let dir = Dir(std::env::temp_dir().join(format!("podrunner-cpu-{}-{n}", std::process::id())));
    let _ = std::fs::remove_dir_all(&dir.0);
    let (unit, reference) = driver(kind, &dir.0);
    let image = unit.prepare(&reference, platform(), false).unwrap();
    let id = UnitId::parse(&format!("u-{n}")).unwrap();
    let spec = StartSpec {
        unit: id.clone(),
        image,
        argv: ["sh", "-c", script].map(String::from).to_vec(),
        env: Default::default(),
        credential: Default::default(),
        sockets: None,
        mounts: Vec::new(),
        limits: Default::default(),
    };
    unit.start(&spec).unwrap();
    let mut usage = Usage::default();
    let end = Instant::now() + Duration::from_secs(600);
    let exit = loop {
        usage.sample(&unit.observe(&id).map(|o| o.pids).unwrap_or_default());
        if let Some(exit) = unit.wait(&id, TICK).unwrap() {
            break Some(exit);
        }
        if Instant::now() > end {
            break None;
        }
    };
    let log = String::from_utf8_lossy(&unit.unit_logs(&id).unwrap_or_default()).to_string();
    let residue = unit.destroy(&id).unwrap();
    let [(cpu, why), _] = usage.report();
    println!("{kind} unit {n}: {exit:?} cpu_s {cpu:?} {why:?}\n{log}");
    assert!(residue.is_clean(), "{residue:?}");
    assert_eq!(exit, Some(Exit::Code(0)), "the payload ran: {log}");
    let load = load_of(&log).unwrap_or_else(|| panic!("no output of times: {log}"));
    let Residual::Count(cpu_s) = cpu else { panic!("cpu_s unknown: {why:?}") };
    Measured { load, cpu_s }
}

#[test]
#[ignore]
fn cpu_s_of_a_known_load_is_within_the_tolerance() {
    let Ok(kind) = std::env::var("PODRUNNER_CPU_KIND") else {
        println!("skip: PODRUNNER_CPU_KIND is not set");
        return;
    };
    let control = measure(&kind, 1, IDLE);
    let ran = measure(&kind, 2, LOAD);
    println!(
        "{kind}: the load {:.2} s of CPU: cpu_s {}; the control {:.2} s: cpu_s {}",
        ran.load, ran.cpu_s, control.load, control.cpu_s
    );
    assert!(ran.load >= 4.0, "the payload burned {:.2} s of CPU, not about 8 s", ran.load);
    // The emulator of a guest also burns CPU for the boot and the stop.
    let cpu_s = if kind == "microvm" { ran.cpu_s.saturating_sub(control.cpu_s) } else { ran.cpu_s };
    assert!(within(cpu_s, ran.load), "{kind}: cpu_s {cpu_s} for {:.2} s of CPU of the payload", ran.load);
}

/// The edges of the tolerance and of the parse of `times`.
#[test]
fn the_tolerance_is_20_percent_or_1_s_the_larger() {
    for (cpu_s, load, want) in [
        (3, 3.0, true),
        (4, 3.0, true),
        (2, 3.0, true),
        (5, 3.0, false),
        (1, 3.0, false),
        (0, 0.4, true),
        (2, 0.4, true),
        (3, 0.4, false),
        (12, 10.0, true),
        (8, 10.0, true),
        (13, 10.0, false),
        (7, 10.0, false),
        (7, 8.19, true),
        (9, 8.19, true),
        (10, 8.19, true),
        (11, 8.19, false),
        (6, 4.97, true),
        (7, 4.97, false),
    ] {
        assert_eq!(within(cpu_s, load), want, "cpu_s {cpu_s} for {load} s");
    }
    let dash = "boot\npodrunner-times\r\n0m0.01s 0m0.02s\r\n0m8.01s 0m0.10s\r\n";
    assert!(load_of(dash).is_some_and(|l| (l - 8.11).abs() < 1e-9), "{:?}", load_of(dash));
    let noisy = "podrunner-times\n[ 9.1] a line of the kernel\n0m0.001s 0m0.002s\n1m2.500s 0m0.500s\n";
    assert!(load_of(noisy).is_some_and(|l| (l - 63.0).abs() < 1e-9), "{:?}", load_of(noisy));
    assert_eq!(load_of("podrunner-times\n0m0.01s 0m0.02s\n"), None, "one line of times");
    assert_eq!(load_of("0m0.01s 0m0.02s\n0m8.01s 0m0.10s\n"), None, "no mark");
    assert_eq!(secs("8.01s"), None);
}
