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

use crate::serve::{Failure, fail};
use crate::text::display;

mod pins;

/// The probes of `pod-hostprobe` that the drivers need: user namespaces
/// and mount (a driver with namespaces), chroot, KVM and Landlock.
pub const HOST_ROWS: [&str; 5] = [
    "unshare(CLONE_NEWUSER)",
    // A mount in a user namespace with an ID map, as a driver with no root
    // does it (T-283, PodUtils/podcrates#201).
    pod_hostprobe::probes::USERNS_LEG,
    // The chroot of a driver with no root, after the same map (T-283,
    // PodUtils/podcrates#204).
    pod_hostprobe::probes::USERNS_CHROOT,
    "open(/dev/kvm, O_RDWR)",
    "landlock_create_ruleset(VERSION)",
];

/// The settings of the probe children: `main` reads the flag first.
pub fn probe_settings() -> pod_hostprobe::Settings {
    pod_hostprobe::Settings {
        child_flag: "--podrunner-probe-child".into(),
        child_name: "podrunner-probe".into(),
        directory_setting: None,
        // The doctor sends no packet past the host.
        network: None,
    }
}

/// Run the rows of [`HOST_ROWS`], each in a child of this binary.
fn host_rows() -> Value {
    use pod_hostprobe::probes::find;
    let spawner = pod_hostprobe::child::Spawner::new(&probe_settings());
    let rows: Vec<Value> = HOST_ROWS
        .iter()
        .filter_map(|name| {
            let o = find(name)?.measure(&spawner);
            Some(json!({ "probe": name, "verdict": o.verdict.word(), "errno": o.errno_name(), "reason": o.reason }))
        })
        .collect();
    json!(rows)
}

/// Read the doctor report also when podnet finds no usable backend.
fn read_podnet_hold(tool: &Path) -> Result<Value, String> {
    let out = Command::new(tool)
        .args(["pod", "doctor", "--json"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("{}: not run ({})", tool.display(), e))?;
    if !out.status.success() && out.status.code() != Some(3) {
        return Err(format!("{} pod doctor --json: exit {}", tool.display(), out.status.code().unwrap_or(-1)));
    }
    serde_json::from_slice(&out.stdout).map_err(|_| format!("{} pod doctor --json: not JSON", tool.display()))
}

/// The capability record of the unit kind `name`, through its driver.
pub(crate) fn capability(name: &str, u: &podrunner_slots::config::UnitConfig) -> Result<Capability, String> {
    capability_at(name, u, &crate::incusd::socket(), &Default::default())
}

/// A podvm kind with `confine`: the self-test of the layer with the
/// pinned image, as `serve` runs it, so that doctor names the boundary
/// that `serve` will have (T-566), or why the layer is not proven.
fn confined(
    name: &str,
    u: &podrunner_slots::config::UnitConfig,
    cfg: &Config,
    c: Capability,
) -> Result<Capability, String> {
    let pin = cfg.pin.first_for(std::env::consts::ARCH, u.init);
    let (true, true, Some(pin)) = (name == "podvm", u.confine, pin) else { return Ok(c) };
    let (binary, images) = (u.binary.clone().unwrap_or_default(), u.images.clone().unwrap_or_default());
    crate::podvm_kind::confine_proven(&binary, &images, u.podnet.as_deref(), &pin.reference)?;
    Ok(Capability { boundary: podrunner_schema::Boundary::Confined, ..c })
}

/// An incusd takes a connection of this user (T-534): the probe of the
/// incus kind reads only the version of podvm.
pub(crate) fn incusd() -> Result<(), String> {
    crate::incusd::connect(&crate::incusd::socket())
}

/// The entry of a kind in the report. An engine on the incus kind gets a
/// warning when the Incus version refuses the runc of the engine, or when
/// incusd gives no version (T-683).
fn kind_entry(name: &str, c: &Capability, incusd: &Path, apparmor: &Path) -> Value {
    let engine = c.container == podrunner_schema::ContainerRoute::Engine;
    let warnings: Vec<String> = if name == "incus" && engine {
        match crate::incusd::engine_check(incusd, apparmor) {
            Ok(w) => w.into_iter().collect(),
            Err(why) => vec![format!(
                "{why}: the version of Incus is unknown; an engine with runc 1.3.3 or later needs Incus 6.0.6 \
                 or later of the 6.0 line, or 6.19 or later (lxc/incus#2623)"
            )],
        }
    } else {
        vec![]
    };
    json!({ "kind": name, "capability": c, "warnings": warnings })
}

/// The record of a kind, with the socket of incusd at `incusd`.
fn capability_at(
    name: &str,
    u: &podrunner_slots::config::UnitConfig,
    incusd: &Path,
    engines: &std::collections::BTreeSet<String>,
) -> Result<Capability, String> {
    use podrunner_unit::UnitDriver;
    match name {
        "fake" => Ok(crate::serve::fake_capability_of(u)),
        "podvm" => {
            let binary = u.binary.as_ref().ok_or("[unit.podvm] names no binary")?;
            let podvm = podrunner_podvm::Podvm { binary: binary.into() };
            // The hold of `podnet pod doctor`, as `serve` reads it (T-181);
            // the confine layer is proven by the self-test of `serve` only.
            let Some(podnet) = &u.podnet else { return podvm.probe().map_err(|e| e.to_string()) };
            let images = std::path::PathBuf::from(u.images.clone().unwrap_or_default());
            let state = std::env::temp_dir().join("podrunner-doctor");
            podrunner_podvm::PodvmUnit::new(podvm, &state, &images)
                .and_then(|unit| unit.with_podnet(std::path::Path::new(podnet)))
                .and_then(|unit| unit.probe())
                .map_err(|e| e.to_string())
        }
        // The probe of a guest kind starts no unit and no process (unit
        // test 23). `serve` takes a claim of `guest` only for a kind on
        // `guest_kinds` and after the self-test of GitHub #36.
        "microvm" => {
            crate::tiers::kvm()?;
            let state = std::env::temp_dir().join("podrunner-doctor");
            // The probe reads no size of an image.
            crate::microvm_kind::build(u, &state, engines.clone(), Default::default())
                .map(|(_, cap)| cap)
                .map_err(|f| f.detail)
        }
        "incus" => {
            let state = std::env::temp_dir().join("podrunner-doctor");
            let (_, cap) = crate::incus_kind::build(u, &state, engines.clone()).map_err(|f| f.detail)?;
            // The probe of the kind reads only the version of podvm: an
            // incusd must also take a connection of this user (T-534).
            crate::incusd::connect(incusd)?;
            Ok(cap)
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
    let mut pins = Vec::new();
    if let Some(path) = config {
        let text = std::fs::read_to_string(path)
            .map_err(|e| fail(Class::Config, format!("the configuration {}: {e}", path.display())))?;
        let cfg = Config::parse(&text).map_err(|e| fail(Class::Config, e))?;
        let mut caps = Vec::new();
        for (name, u) in &cfg.unit {
            let engines = cfg.pin.payload_engine_digests(std::env::consts::ARCH);
            let incusd = crate::incusd::socket();
            match capability_at(name, u, &incusd, &engines).and_then(|c| confined(name, u, &cfg, c)) {
                Ok(c) => {
                    kinds.push(kind_entry(name, &c, &incusd, Path::new(crate::incusd::APPARMOR)));
                    caps.push(c);
                }
                Err(why) => {
                    kinds.push(json!({ "kind": name, "missing": why }));
                    missing.push(format!("[unit.{name}]: {why}"));
                }
            }
            if let Some(podnet) = &u.podnet {
                hold = read_podnet_hold(Path::new(podnet)).unwrap_or_else(|why| {
                    missing.push(format!("the hold of [unit.{name}] podnet: {why}"));
                    Value::Null
                });
            }
        }
        for s in &cfg.source {
            let floor = cfg.floor(s.tier);
            let need = podrunner_slots::placement::needed_hold(s.tier);
            // Why for each kind (T-194): an emulated host, for example,
            // has no confine layer and no hold.
            let why = crate::tiers::not_served(&cfg, s, &caps);
            // The extra permissions of the App: of a personal account (T-504).
            let extra: Vec<&str> =
                s.allow_personal_account.then_some(podrunner_slots::PERSONAL_PERMISSION).into_iter().collect();
            tiers.push(json!({
                "source": s.name, "tier": s.tier, "served": why.is_none(), "floor": floor, "hold": need,
                "extra_permissions": extra,
            }));
            if let Some(why) = why {
                missing.push(format!(
                    "[[source]] {}: no unit kind meets the floor and the hold of its tier ({why})",
                    s.name
                ));
            }
        }
        // A pinned image larger than the bound of a kind, as `check`
        // refuses it (T-551, T-573).
        for (kind, u) in &cfg.unit {
            if let Err(f) = crate::pin_size::kind_fits(kind, u, &cfg.pin) {
                missing.push(f.detail);
            }
        }
        pins = pins::rows(&cfg, pins::now_s());
    } else {
        missing.push(
            "--config: no unit kind and no source were read; give podrunner doctor --run FILE (a workflow of run) \
             or podrunner doctor --config FILE (a configuration of serve)"
                .into(),
        );
    }
    Ok(json!({
        "schema": 1, "landlock_abi": landlock, "kvm": kvm, "hold": hold, "host": host,
        "kinds": kinds, "sources": tiers, "pins": pins, "missing": missing,
        "runner_of_this_build": pins::RUNNER_OF_THIS_BUILD,
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
            for warning in k["warnings"].as_array().into_iter().flatten() {
                let warning = crate::text::display_max(warning.as_str().unwrap_or(""), crate::text::MAX_LINE);
                out.push_str(&format!("kind {name}: warning: {warning}\n"));
            }
        } else if let Some(why) = k["missing"].as_str() {
            // serve drops the kind (T-629): say so with the cause.
            out.push_str(&format!("kind {name}: not served on this host: {}\n", display(why)));
        }
    }
    for s in v["sources"].as_array().into_iter().flatten() {
        let word = |f: &str| display(s[f].as_str().unwrap_or("?"));
        let served = if s["served"] == json!(true) { "served" } else { "not served" };
        let extra: Vec<String> =
            s["extra_permissions"].as_array().into_iter().flatten().filter_map(|p| p.as_str()).map(display).collect();
        let extra = match extra.is_empty() {
            true => String::new(),
            false => format!("; the App needs {} (allow_personal_account)", extra.join(", ")),
        };
        out.push_str(&format!("source {}: tier {}, {served}{extra}\n", word("source"), word("tier")));
    }
    if let Some(r) = v["runner_of_this_build"].as_str() {
        out.push_str(&format!("runner of this build: {}\n", display(r)));
    }
    out.push_str(&pins::lines(v));
    for m in v["missing"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "missing: {}\n",
            crate::text::display_max(m.as_str().unwrap_or(""), crate::text::MAX_LINE)
        ));
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
mod tests;
