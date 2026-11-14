//! `podrunner doctor [--config FILE] [--json]` (T-070): a thin reader of
//! the checks that other tools own (call C9, DR-15). With a configuration,
//! the capability record of each unit kind through its driver binary (the
//! record that placement reads), and the tiers that the host can serve
//! with it. Always: the Landlock ABI (`pod-confine`) and `/dev/kvm`; the
//! hold of `podnet pod doctor --json` when `[unit.podvm] podnet` names it,
//! and the rows of `pod-hostprobe` that the drivers need ([`HOST_ROWS`]),
//! each in a disposable child of this binary.
//! It names each input that is missing; it decides nothing and changes no
//! file.

use std::path::Path;
use std::process::{Command, Stdio};

use pod_core::failure::Class;
use podrunner_schema::Capability;
use podrunner_slots::config::Config;
use serde_json::{Value, json};

use crate::serve::{Failure, fail, fake_capability};
use crate::text::display;

/// The probes of `pod-hostprobe` that the drivers need: user namespaces
/// and mount (a driver with namespaces), chroot, KVM and Landlock.
pub const HOST_ROWS: [&str; 5] = [
    "unshare(CLONE_NEWUSER)",
    "mount(tmpfs,/mnt) in clone(NEWNS)",
    "chroot(/tmp)",
    "open(/dev/kvm, O_RDWR)",
    "landlock_create_ruleset(VERSION)",
];

/// The settings of the probe children: `main` reads the flag first.
pub fn probe_settings() -> pod_hostprobe::Settings {
    pod_hostprobe::Settings {
        child_flag: "--podrunner-probe-child".into(),
        child_name: "podrunner-probe".into(),
        directory_setting: None,
    }
}

/// Run the rows of [`HOST_ROWS`], each in a child of this binary.
fn host_rows() -> Value {
    use pod_hostprobe::probes::{Kind, find};
    let spawner = pod_hostprobe::child::Spawner::new(&probe_settings());
    let rows: Vec<Value> = HOST_ROWS
        .iter()
        .filter_map(|name| {
            let o = match find(name)?.kind {
                Kind::Clone(flags) => spawner.clone_accepted(flags),
                Kind::Child { ns_flags, .. } => spawner.run_probe(name, ns_flags),
            };
            Some(json!({ "probe": name, "verdict": o.verdict.word(), "errno": o.errno_name(), "reason": o.reason }))
        })
        .collect();
    json!(rows)
}

/// Run one tool and read its JSON, or say why there is none.
fn read(tool: &Path, args: &[&str]) -> Result<Value, String> {
    let out = Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("{}: not run ({})", tool.display(), e.kind()))?;
    if !out.status.success() {
        return Err(format!("{} {}: exit {}", tool.display(), args.join(" "), out.status.code().unwrap_or(-1)));
    }
    serde_json::from_slice(&out.stdout).map_err(|_| format!("{} {}: not JSON", tool.display(), args.join(" ")))
}

/// The capability record of the unit kind `name`, through its driver.
fn capability(name: &str, u: &podrunner_slots::config::UnitConfig) -> Result<Capability, String> {
    use podrunner_unit::UnitDriver;
    match name {
        "fake" => Ok(fake_capability()),
        "podvm" => {
            let binary = u.binary.as_ref().ok_or("[unit.podvm] names no binary")?;
            podrunner_podvm::Podvm { binary: binary.into() }.probe().map_err(|e| e.to_string())
        }
        // The probe of a guest kind starts no unit and no process (unit
        // test 23). `serve` takes a claim of `guest` only for a kind on
        // `guest_kinds` and after the self-test of GitHub #36.
        "microvm" => {
            crate::microvm_kind::plan(u).map_err(|f| f.detail)?;
            let podvm = podrunner_podvm::Podvm { binary: u.binary.clone().unwrap_or_default().into() };
            let images = std::path::PathBuf::from(u.images.clone().unwrap_or_default());
            let state = std::env::temp_dir().join("podrunner-doctor");
            podrunner_microvm::MicrovmUnit::new(podvm, &state, &images).probe().map_err(|e| e.to_string())
        }
        "incus" => {
            crate::incus_kind::plan(u).map_err(|f| f.detail)?;
            let podvm = podrunner_podvm::Podvm { binary: u.binary.clone().unwrap_or_default().into() };
            let images = std::path::PathBuf::from(u.images.clone().unwrap_or_default());
            let state = std::env::temp_dir().join("podrunner-doctor");
            podrunner_incus::IncusUnit::new(podvm, &state, &images).probe().map_err(|e| e.to_string())
        }
        other => Err(format!("no driver of the kind {other} in this build")),
    }
}

/// The report as JSON: the findings, and each input that is missing.
pub fn report(config: Option<&Path>) -> Result<Value, Failure> {
    report_with(config, host_rows())
}

/// The report with the rows of the host given (a unit test runs no probe
/// child: its binary is the test harness).
fn report_with(config: Option<&Path>, host: Value) -> Result<Value, Failure> {
    let mut missing: Vec<String> = Vec::new();
    let landlock = match podrunner_unit::confine::check() {
        Ok(abi) => json!(abi),
        Err(why) => {
            missing.push(format!("Landlock: {why}"));
            Value::Null
        }
    };
    let kvm = std::fs::OpenOptions::new().read(true).write(true).open("/dev/kvm").is_ok();
    let mut kinds = Vec::new();
    let mut tiers = Vec::new();
    let mut hold = Value::Null;
    if let Some(path) = config {
        let text = std::fs::read_to_string(path)
            .map_err(|e| fail(Class::Config, format!("the configuration: {}", e.kind())))?;
        let cfg = Config::parse(&text).map_err(|e| fail(Class::Config, e))?;
        let mut caps = Vec::new();
        for (name, u) in &cfg.unit {
            match capability(name, u) {
                Ok(c) => {
                    kinds.push(json!({ "kind": name, "capability": c }));
                    caps.push(c);
                }
                Err(why) => {
                    kinds.push(json!({ "kind": name, "missing": why }));
                    missing.push(format!("[unit.{name}]: {why}"));
                }
            }
            if let Some(podnet) = &u.podnet {
                hold = read(Path::new(podnet), &["pod", "doctor", "--json"]).unwrap_or_else(|why| {
                    missing.push(format!("the hold of [unit.{name}] podnet: {why}"));
                    Value::Null
                });
            }
        }
        for s in &cfg.source {
            let floor = cfg.floor(s.tier);
            let need = podrunner_slots::placement::needed_hold(s.tier);
            let served =
                caps.iter().any(|c| c.arch == s.arch && c.boundary >= floor && c.egress_hold.effective() >= need);
            tiers.push(json!({
                "source": s.name, "tier": s.tier, "served": served, "floor": floor, "hold": need,
            }));
            if !served {
                missing.push(format!("[[source]] {}: no unit kind meets the floor and the hold of its tier", s.name));
            }
        }
    } else {
        missing.push("--config: no unit kind and no source were read".into());
    }
    Ok(json!({
        "schema": 1, "landlock_abi": landlock, "kvm": kvm, "hold": hold, "host": host,
        "kinds": kinds, "sources": tiers, "missing": missing,
    }))
}

/// The report as text: one line for each finding, then each missing
/// input.
pub fn text(v: &Value) -> String {
    let mut out = String::new();
    match v["landlock_abi"].as_u64() {
        Some(abi) => out.push_str(&format!("landlock: abi {abi}\n")),
        None => out.push_str("landlock: none\n"),
    }
    out.push_str(&format!("kvm: {}\n", if v["kvm"] == json!(true) { "yes" } else { "no" }));
    for r in v["host"].as_array().into_iter().flatten() {
        let w = |f: &str| display(r[f].as_str().unwrap_or(""));
        let errno = r["errno"].as_str().map(|e| format!(" ({})", display(e))).unwrap_or_default();
        out.push_str(&format!("probe {}: {}{errno}\n", w("probe"), w("verdict")));
    }
    for backend in ["netns", "seccomp"] {
        let level = v["hold"]["hold"][backend]["level"].as_str().or_else(|| v["hold"][backend]["level"].as_str());
        if let Some(level) = level {
            out.push_str(&format!("hold {backend}: {}\n", display(level)));
        }
    }
    for k in v["kinds"].as_array().into_iter().flatten() {
        let name = display(k["kind"].as_str().unwrap_or(""));
        let c = &k["capability"];
        if c.is_object() {
            let w = |f: &str| display(c[f].as_str().unwrap_or("?"));
            out.push_str(&format!(
                "kind {name}: boundary {}, hold {}, container {}, tree kill {}\n",
                w("boundary"),
                w("egress_hold"),
                w("container"),
                w("tree_kill")
            ));
        }
    }
    for s in v["sources"].as_array().into_iter().flatten() {
        let word = |f: &str| display(s[f].as_str().unwrap_or("?"));
        let served = if s["served"] == json!(true) { "served" } else { "not served" };
        out.push_str(&format!("source {}: tier {}, {served}\n", word("source"), word("tier")));
    }
    for m in v["missing"].as_array().into_iter().flatten() {
        out.push_str(&format!("missing: {}\n", display(m.as_str().unwrap_or(""))));
    }
    out
}

/// The tier words of a source in the report, for a test.
#[cfg(test)]
fn tier_of(v: &Value, source: &str) -> Option<(podrunner_schema::Tier, bool)> {
    let s = v["sources"].as_array()?.iter().find(|s| s["source"] == json!(source))?;
    Some((serde_json::from_value(s["tier"].clone()).ok()?, s["served"] == json!(true)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use podrunner_schema::Tier;

    #[test]
    fn a_missing_tool_is_named() {
        let r = read(Path::new("podrunner-no-such-tool"), &["--json"]).unwrap_err();
        assert!(r.starts_with("podrunner-no-such-tool: not run"), "{r}");
    }

    /// T-113: the guest kinds give their record through their probe.
    #[test]
    fn doctor_knows_the_microvm_and_incus_kinds() {
        let dir = std::env::temp_dir().join(format!("podrunner-doctor-guest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let u = |extra: &str| -> podrunner_slots::config::UnitConfig {
            toml::from_str(&format!("slots = 1\nbinary = \"/bin/true\"\nimages = \"{}\"\n{extra}", dir.display()))
                .unwrap()
        };
        let m = capability("microvm", &u("")).unwrap();
        assert_eq!((m.kind.as_str(), m.boundary), ("microvm", podrunner_schema::Boundary::Guest));
        let i = capability("incus", &u("")).unwrap();
        assert_eq!((i.kind.as_str(), i.boundary), ("incus", podrunner_schema::Boundary::Confined));
        assert!(capability("incus", &u("podnet = \"/bin/true\"")).unwrap_err().contains("podnet"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn doctor_reads_the_kinds_and_the_tiers_of_the_file() {
        let dir = std::env::temp_dir().join(format!("podrunner-doctor-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("podrunner.toml");
        let arch = std::env::consts::ARCH;
        std::fs::write(
            &file,
            format!(
                "host = \"h\"\nallow_fake = true\n[[source]]\nname = \"local\"\nkind = \"fake\"\ntier = \"local\"\n\
                 arch = \"{arch}\"\n[[source]]\nname = \"org\"\nkind = \"fake\"\ntier = \"trusted\"\narch = \"{arch}\"\n\
                 [unit.fake]\nslots = 1\n[trust]\norganizations = [{{ login = \"example-org\" }}]\n[pin]\n"
            ),
        )
        .unwrap();
        let v = report_with(Some(&file), json!([])).unwrap();
        // The fake kind is `none`: it serves `local`, never `trusted`.
        assert_eq!(tier_of(&v, "local"), Some((Tier::Local, true)));
        assert_eq!(tier_of(&v, "org"), Some((Tier::Trusted, false)));
        let t = text(&v);
        assert!(t.contains("kind fake: boundary none"), "{t}");
        assert!(t.contains("source org: tier trusted, not served"), "{t}");
        assert!(t.contains("missing: [[source]] org"), "{t}");
        // With no file, doctor says what it did not read.
        let bare = report_with(None, json!([])).unwrap();
        assert!(text(&bare).contains("missing: --config"), "{}", text(&bare));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
