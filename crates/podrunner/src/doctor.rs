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

/// Run one tool and read its JSON, or say why there is none.
fn read(tool: &Path, args: &[&str]) -> Result<Value, String> {
    let out = Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("{}: not run ({})", tool.display(), e))?;
    if !out.status.success() {
        return Err(format!("{} {}: exit {}", tool.display(), args.join(" "), out.status.code().unwrap_or(-1)));
    }
    serde_json::from_slice(&out.stdout).map_err(|_| format!("{} {}: not JSON", tool.display(), args.join(" ")))
}

/// The capability record of the unit kind `name`, through its driver.
pub(crate) fn capability(name: &str, u: &podrunner_slots::config::UnitConfig) -> Result<Capability, String> {
    capability_at(name, u, &incusd_socket())
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

/// The socket of incusd, as podvm finds it: `PODVM_INCUS_SOCKET`, else
/// `unix.socket` of `INCUS_DIR`, else of `/var/lib/incus`.
fn incusd_socket() -> std::path::PathBuf {
    if let Some(s) = std::env::var_os("PODVM_INCUS_SOCKET") {
        return s.into();
    }
    let dir = std::env::var_os("INCUS_DIR").map_or_else(|| "/var/lib/incus".into(), std::path::PathBuf::from);
    dir.join("unix.socket")
}

/// An incusd takes a connection of this user (T-534): the probe of the
/// incus kind reads only the version of podvm.
pub(crate) fn incusd() -> Result<(), String> {
    connect_incusd(&incusd_socket())
}

fn connect_incusd(socket: &Path) -> Result<(), String> {
    std::os::unix::net::UnixStream::connect(socket)
        .map(drop)
        .map_err(|e| format!("incusd: {}: {}", socket.display(), e.kind()))
}

/// The record of a kind, with the socket of incusd at `incusd`.
fn capability_at(name: &str, u: &podrunner_slots::config::UnitConfig, incusd: &Path) -> Result<Capability, String> {
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
            crate::microvm_kind::plan(u).map_err(|f| f.detail)?;
            let podvm = podrunner_podvm::Podvm { binary: u.binary.clone().unwrap_or_default().into() };
            let images = std::path::PathBuf::from(u.images.clone().unwrap_or_default());
            let state = std::env::temp_dir().join("podrunner-doctor");
            podrunner_microvm::MicrovmUnit::new(podvm, &state, &images).probe().map_err(|e| e.to_string())
        }
        "incus" => {
            crate::incus_kind::plan(u).map_err(|f| f.detail)?;
            // The probe of the kind reads only the version of podvm: an
            // incusd must also take a connection of this user (T-534).
            connect_incusd(incusd)?;
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
    let mut pins = Vec::new();
    if let Some(path) = config {
        let text = std::fs::read_to_string(path)
            .map_err(|e| fail(Class::Config, format!("the configuration {}: {e}", path.display())))?;
        let cfg = Config::parse(&text).map_err(|e| fail(Class::Config, e))?;
        let mut caps = Vec::new();
        for (name, u) in &cfg.unit {
            match capability(name, u).and_then(|c| confined(name, u, &cfg, c)) {
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
            let mut row = json!({
                "source": s.name, "tier": s.tier, "served": served, "floor": floor, "hold": need,
            });
            // The extra permission of the App of a personal account (T-504).
            if s.allow_personal_account {
                row["extra_permission"] = json!(podrunner_slots::PERSONAL_PERMISSION);
            }
            tiers.push(row);
            if !served {
                // Why for each kind (T-194): an emulated host, for example,
                // has no confine layer and no hold.
                let why: Vec<String> = caps
                    .iter()
                    .filter(|c| c.arch == s.arch)
                    .map(|c| {
                        let mut w = Vec::new();
                        if c.boundary < floor {
                            w.push(format!("boundary {:?} below {floor:?}", c.boundary));
                        }
                        if c.egress_hold.effective() < need {
                            w.push(format!("hold {:?} below {need:?}", c.egress_hold.effective()));
                        }
                        format!("{}: {}", c.kind, w.join(", "))
                    })
                    .collect();
                let why = if why.is_empty() { format!("no unit kind of {}", s.arch) } else { why.join("; ") };
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
        missing.push("--config: no unit kind and no source were read".into());
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
        }
    }
    for s in v["sources"].as_array().into_iter().flatten() {
        let word = |f: &str| display(s[f].as_str().unwrap_or("?"));
        let served = if s["served"] == json!(true) { "served" } else { "not served" };
        let extra = match s["extra_permission"].as_str() {
            Some(p) => format!("; the App needs {} (allow_personal_account)", display(p)),
            None => String::new(),
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
mod tests {
    use super::*;
    use podrunner_schema::Tier;

    #[test]
    fn a_pin_names_its_runner_its_age_and_the_end_of_its_window() {
        let day = 24 * 3600;
        let cfg = Config::parse(&format!(
            "host = \"h\"\n[pin]\n[[pin.payload]]\narch = \"x86_64\"\nreference = \"runner@sha256:{d}\"\n\
             built_at_s = {a}\nrunner_version = \"2.338.2\"\n[[pin.payload]]\narch = \"aarch64\"\n\
             reference = \"runner@sha256:{d}\"\nbuilt_at_s = {b}\n[[pin.payload]]\narch = \"x86_64\"\n\
             reference = \"runner@sha256:{e}\"\nbuilt_at_s = {c}\n",
            d = "1".repeat(64),
            e = "2".repeat(64),
            a = 100 * day - 2 * day,
            b = 100 * day - 25 * day,
            c = 100 * day - 31 * day,
        ))
        .unwrap();
        let rows = pins::rows(&cfg, 100 * day);
        let states: Vec<&str> = rows.iter().map(|r| r["state"].as_str().unwrap()).collect();
        assert_eq!(states, ["ok", "near-end", "stale"]);
        let text = text(&json!({ "pins": rows, "runner_of_this_build": pins::RUNNER_OF_THIS_BUILD }));
        assert!(text.contains("pin x86_64: runner 2.338.2, 2 of 30 days\n"), "{text}");
        assert!(text.contains("pin aarch64: runner not named, 25 of 30 days: near the end of its window"), "{text}");
        assert!(text.contains("pin x86_64: runner not named, 31 of 30 days: stale"), "{text}");
        let built = pins::RUNNER_OF_THIS_BUILD;
        assert!(built.starts_with("2."), "{built}");
        assert!(text.contains(&format!("runner of this build: {built}\n")), "{text}");
    }

    /// T-551: doctor names the size of a pinned image, the host disk of the
    /// incus kind for it, and, as `check`, an image over the root disk.
    #[test]
    fn doctor_names_the_disk_of_a_pinned_image() {
        let dir = crate::scratch::Scratch::new("doctor-disk");
        let file = dir.join("podrunner.toml");
        let pin = |size: u64| {
            format!(
                "host = \"h\"\n[unit.incus]\nslots = 1\nbinary = \"/bin/true\"\nimages = \"/\"\nroot_quota = true\n\
                 disk_max_bytes = 40000000000\n[pin]\n[[pin.payload]]\narch = \"{}\"\n\
                 reference = \"hosted@sha256:{}\"\nbuilt_at_s = 1\nunpacked_bytes = {size}\n",
                std::env::consts::ARCH,
                "1".repeat(64)
            )
        };
        std::fs::write(&file, pin(55_000_000_000)).unwrap();
        let v = report_with(Some(&file), json!([])).unwrap();
        assert_eq!(v["pins"][0]["unpacked_bytes"], json!(55_000_000_000u64), "{v}");
        assert_eq!(v["pins"][0]["incus_host_bytes"], json!(187_000_000_000u64), "{v}");
        let t = text(&v);
        assert!(t.contains(", 55.0 GB unpacked, the incus kind needs about 187.0 GB of host disk"), "{t}");
        assert!(t.contains("missing: [unit.incus] disk_max_bytes: the pin hosted@sha256:"), "{t}");
        assert!(t.contains("fix: set disk_max_bytes above 55.0 GB"), "{t}");
        std::fs::write(&file, pin(30_000_000_000)).unwrap();
        let v = report_with(Some(&file), json!([])).unwrap();
        assert!(!v["missing"].to_string().contains("disk_max_bytes"), "{v}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_tool_is_named() {
        let r = read(Path::new("podrunner-no-such-tool"), &["--json"]).unwrap_err();
        assert!(r.starts_with("podrunner-no-such-tool: not run"), "{r}");
    }

    /// T-113: the guest kinds give their record through their probe.
    #[test]
    fn doctor_knows_the_microvm_and_incus_kinds() {
        use std::os::unix::fs::PermissionsExt;
        let dir = crate::scratch::Scratch::new("doctor-guest");
        // A fake podvm whose probe names a version that each kind takes.
        let fake = dir.join("podvm");
        std::fs::write(&fake, "#!/bin/sh\necho '{\"podvm\":\"0.2.0-beta.14\",\"rung\":\"userland\"}'\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let u = |extra: &str| -> podrunner_slots::config::UnitConfig {
            toml::from_str(&format!(
                "slots = 1\nbinary = \"{}\"\nimages = \"{}\"\n{extra}",
                fake.display(),
                dir.display()
            ))
            .unwrap()
        };
        let m = capability("microvm", &u("")).unwrap();
        assert_eq!((m.kind.as_str(), m.boundary), ("microvm", podrunner_schema::Boundary::Guest));
        // T-534: no incusd, no incus kind.
        let incusd = dir.join("incus.socket");
        let _ = std::fs::remove_file(&incusd);
        assert!(capability_at("incus", &u(""), &incusd).unwrap_err().starts_with("incusd: "));
        let _listen = std::os::unix::net::UnixListener::bind(&incusd).unwrap();
        let i = capability_at("incus", &u(""), &incusd).unwrap();
        assert_eq!((i.kind.as_str(), i.boundary), ("incus", podrunner_schema::Boundary::Confined));
        assert!(capability_at("incus", &u("podnet = \"/bin/true\""), &incusd).unwrap_err().contains("podnet"));
        // T-181: the podvm kind with podnet gives the hold of `podnet pod
        // doctor`, as `serve` does; with none, `unknown`.
        let podnet = dir.join("podnet");
        std::fs::write(
            &podnet,
            "#!/bin/sh\necho '{\"seccomp\":{\"level\":\"advisory\"},\"netns\":{\"level\":\"none\"}}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&podnet, std::fs::Permissions::from_mode(0o755)).unwrap();
        let held = capability("podvm", &u(&format!("podnet = \"{}\"", podnet.display()))).unwrap();
        assert_eq!(held.egress_hold, podrunner_schema::EgressHold::Advisory);
        assert_eq!(capability("podvm", &u("")).unwrap().egress_hold, podrunner_schema::EgressHold::Unknown);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn doctor_reads_the_kinds_and_the_tiers_of_the_file() {
        let dir = crate::scratch::Scratch::new("doctor");
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
        // T-194: the line names the shortfall of each kind.
        let missing = v["missing"].to_string();
        assert!(missing.contains("fake: boundary None below Confined"), "{missing}");
        let t = text(&v);
        assert!(t.contains("kind fake: boundary none"), "{t}");
        assert!(t.contains("source org: tier trusted, not served"), "{t}");
        assert!(t.contains("missing: [[source]] org"), "{t}");
        // With no file, doctor says what it did not read.
        let bare = report_with(None, json!([])).unwrap();
        assert!(text(&bare).contains("missing: --config"), "{}", text(&bare));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-504: a scale-set source with `allow_personal_account` names the
    /// extra permission of the App; with the key off it names none.
    #[test]
    fn doctor_names_the_extra_permission_of_a_personal_account() {
        let dir = crate::scratch::Scratch::new("doctor-personal");
        let file = dir.join("podrunner.toml");
        let arch = std::env::consts::ARCH;
        for (key, want) in
            [("", false), ("allow_personal_account = false\n", false), ("allow_personal_account = true\n", true)]
        {
            std::fs::write(
                &file,
                format!(
                    "host = \"h\"\n[[source]]\nname = \"alice-app\"\nkind = \"github-scaleset\"\ntier = \"trusted\"\n\
                     arch = \"{arch}\"\nsidecar = \"/bin/true\"\nconfig_url = \"https://github.com/alice/app\"\n\
                     scale_set = \"s\"\ntoken_file = \"/t\"\n{key}[trust]\nusers = [{{ login = \"alice\" }}]\n[pin]\n"
                ),
            )
            .unwrap();
            let v = report_with(Some(&file), json!([])).unwrap();
            let row = &v["sources"][0];
            let named =
                text(&v).contains("source alice-app: tier trusted, not served; the App needs Administration: write");
            assert_eq!(
                (row.get("extra_permission"), named),
                (want.then_some(&json!("Administration: write")), want),
                "{key}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
