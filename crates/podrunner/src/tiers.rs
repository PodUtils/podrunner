//! The check of `check` and `serve` that a kind can serve a source (T-185).

use pod_core::failure::Class;
use podrunner_schema::{Boundary, Capability, EgressHold};
use podrunner_slots::Slots;
use podrunner_slots::config::{Config, SourceConfig, UnitConfig};

use crate::serve::{Failure, fail, fake_capability};
use crate::text::display;

/// A file whose sources no kind can serve is refused, as `serve` says
/// `no-tier-served`. Each kind counts at the best boundary that it can
/// claim, so a self-test of `serve` can only lower it. A kind also needs
/// the egress hold of the tier, as placement does (#280): a podvm kind
/// has a hold only with `podnet`, and then at best `enforced`.
///
/// With `here` false (`check`), each kind counts: the verdict is the same
/// on each host, so a build machine can validate the file of a serve host
/// (T-629). With `here` true (`serve`, `config init`), a kind that this
/// host cannot run counts out, as `serve` drops it. A file that a kind of
/// another host would serve is then a temporary failure (75): the kind can
/// come later (udev gives `/dev/kvm` after the start of the unit), and a
/// supervisor starts `serve` again. A file that no kind serves is 78.
pub fn served(cfg: &Config, here: bool) -> Result<(), Failure> {
    verdict(cfg, if here { away(cfg) } else { Vec::new() })
}

/// [`served`] with the kinds that this host cannot run given (none: the
/// verdict of `check`).
fn verdict(cfg: &Config, away: Vec<(String, String)>) -> Result<(), Failure> {
    let arch = std::env::consts::ARCH;
    let meets = |skip: &[(String, String)]| {
        cfg.source.iter().any(|s| {
            let (floor, need) = (cfg.floor(s.tier), podrunner_slots::placement::needed_hold(s.tier));
            cfg.unit.iter().filter(|(n, _)| !skip.iter().any(|(a, _)| a == *n)).any(|(n, u)| {
                let kind_arch = if n == "fake" { fake_capability().arch } else { arch.to_string() };
                kind_arch == s.arch && best(n, u) >= floor && best_hold(n, u).effective() >= need
            })
        })
    };
    let refuse = |class, why: &str, tail: &str| {
        Err(fail(
            class,
            format!(
                "no-tier-served: no unit kind meets the floor and the egress hold of a source on this host{why}{tail}"
            ),
        ))
    };
    if cfg.source.is_empty() || meets(&away) {
        return Ok(());
    }
    let mut why: String = away.iter().map(|(n, w)| format!(" ([unit.{n}] not here: {w})")).collect();
    why.push_str(&podvm_fix(cfg));
    if meets(&[]) {
        return refuse(Class::TempFail, &why, "; the kind can come later, so a supervisor starts serve again");
    }
    refuse(Class::Config, &why, "")
}

/// The best boundary that the kind `name` can claim, as `check` counts
/// it: a self-test of `serve` can only lower it.
fn best(name: &str, u: &UnitConfig) -> Boundary {
    match name {
        "fake" => crate::serve::fake_capability_of(u).boundary,
        "podvm" if u.confine => Boundary::Confined,
        "podvm" => Boundary::Redirect,
        "incus" => Boundary::Confined,
        _ => Boundary::Guest,
    }
}

/// The best egress hold of the kind `name`: a podvm kind has a hold only
/// with `podnet` (#280).
fn best_hold(name: &str, u: &UnitConfig) -> EgressHold {
    match name {
        "fake" => crate::serve::fake_capability_of(u).egress_hold,
        "podvm" if u.podnet.is_none() => EgressHold::Unknown,
        _ => EgressHold::Enforced,
    }
}

/// Why no kind of `caps` serves the source `s`, or `None` when one does
/// (T-194): each kind of its arch, with its boundary or its hold below the
/// tier, and the proof of a boundary that a self-test lowered. `doctor`
/// and `serve` give this text.
pub fn not_served(cfg: &Config, s: &SourceConfig, caps: &[Capability]) -> Option<String> {
    let (floor, need) = (cfg.floor(s.tier), podrunner_slots::placement::needed_hold(s.tier));
    let mine: Vec<&Capability> = caps.iter().filter(|c| c.arch == s.arch).collect();
    if mine.iter().any(|c| c.boundary >= floor && c.egress_hold.effective() >= need) {
        return None;
    }
    let why: Vec<String> = mine
        .iter()
        .map(|c| {
            let mut w = Vec::new();
            if c.boundary < floor {
                let proof = c.boundary_proof.as_deref().map(|p| format!(" ({})", display(p))).unwrap_or_default();
                w.push(format!("boundary {:?} below {floor:?}{proof}", c.boundary));
            }
            if c.egress_hold.effective() < need {
                w.push(format!("hold {:?} below {need:?}", c.egress_hold.effective()));
            }
            format!("{}: {}", c.kind, w.join(", "))
        })
        .collect();
    Some(if why.is_empty() { format!("no unit kind of {} runs on this host", s.arch) } else { why.join("; ") })
}

/// The line of `serve` when the kinds that it started serve no source
/// (T-712), in the words of `check`: each source with each kind and its
/// cause, each kind that the start of `serve` lowered, and the keys that a
/// podvm kind needs. `None` when a kind serves a source.
pub fn lowered(cfg: &Config, slots: &Slots) -> Option<String> {
    if !slots.no_tier_served() {
        return None;
    }
    let kinds = slots.status().kinds;
    let caps: Vec<Capability> = kinds.iter().map(|k| k.probe.clone()).collect();
    let mut why = String::new();
    for s in &cfg.source {
        if let Some(w) = not_served(cfg, s, &caps) {
            why.push_str(&format!("; [[source]] {}: {w}", s.name));
        }
    }
    for k in &kinds {
        let Some(u) = cfg.unit.get(&k.name) else { continue };
        let (b, h) = (best(&k.name, u), best_hold(&k.name, u).effective());
        if k.probe.boundary < b {
            why.push_str(&format!("; [unit.{}] serve lowered the boundary {b:?} to {:?}", k.name, k.probe.boundary));
        }
        if k.probe.egress_hold.effective() < h {
            why.push_str(&format!(
                "; [unit.{}] serve lowered the hold {h:?} to {:?}",
                k.name,
                k.probe.egress_hold.effective()
            ));
        }
    }
    why.push_str(&podvm_fix(cfg));
    Some(format!("no-tier-served: no unit kind meets the floor and the egress hold of a source on this host{why}"))
}

/// The keys that a podvm kind needs for a source of this architecture
/// that it could serve (T-635): `confine` for the floor `confined`, and
/// `podnet` for an egress hold. `config init` writes `podnet` from
/// `--podnet`.
fn podvm_fix(cfg: &Config) -> String {
    let Some(u) = cfg.unit.get("podvm") else { return String::new() };
    let arch = std::env::consts::ARCH;
    let floors: Vec<Boundary> = cfg.source.iter().filter(|s| s.arch == arch).map(|s| cfg.floor(s.tier)).collect();
    let holds = cfg.source.iter().filter(|s| s.arch == arch && cfg.floor(s.tier) <= Boundary::Confined);
    let mut out = String::new();
    if !u.confine && floors.contains(&Boundary::Confined) {
        out.push_str("; [unit.podvm] has no confine = true: the floor of the source is confined");
    }
    if u.podnet.is_none()
        && holds.into_iter().any(|s| EgressHold::Unknown.effective() < podrunner_slots::placement::needed_hold(s.tier))
    {
        out.push_str(
            "; [unit.podvm] has no podnet: a job of the source needs an egress hold: set podnet = \"PATH\" \
             (config init: --podnet PATH)",
        );
    }
    out
}

/// This host can run the kind `name` (T-629): the microvm kind needs
/// `/dev/kvm`, the incus kind an incusd that takes a connection of this
/// user (T-534). `config init`, `doctor`, `check` and `serve` ask the same
/// question, so none of them offers a kind that `serve` drops.
pub fn here(name: &str) -> Result<(), String> {
    match name {
        "microvm" => kvm(),
        "incus" => crate::doctor::incusd(),
        _ => Ok(()),
    }
}

/// The kinds of `cfg` that this host cannot run, each with its reason.
pub fn away(cfg: &Config) -> Vec<(String, String)> {
    cfg.unit.keys().filter_map(|n| here(n).err().map(|w| (n.clone(), w))).collect()
}

/// `/dev/kvm` opens for reading and writing: a guest needs it, and with
/// no KVM podvm would fetch QEMU and boot nothing.
pub fn kvm() -> Result<(), String> {
    std::fs::OpenOptions::new().read(true).write(true).open("/dev/kvm").map(drop).map_err(|e| {
        format!("/dev/kvm: {e}: the microvm kind needs KVM (a host with KVM, and a user that can open /dev/kvm)")
    })
}

/// A line for each payload built after the clock `now_s` of this host
/// (#287): placement refuses each of its jobs as `image-stale` until then,
/// and the age in `status` is 0, so the cause needs a line of its own.
pub fn future_pins(cfg: &Config, now_s: u64) -> Vec<String> {
    let skew = podrunner_slots::pins::SKEW_S;
    cfg.pin
        .payload
        .iter()
        .filter(|p| p.built_at_s > now_s.saturating_add(skew))
        .map(|p| {
            format!(
                "[[pin.payload]] {}: built_at_s {} is after the clock of this host ({now_s}); each job of this \
                 image is refused as image-stale until then: set the time of the build",
                p.reference, p.built_at_s
            )
        })
        .collect()
}

/// The configuration file holds the signer of the pin list and each path
/// of a key (T-409): a file that the group or others can write, or that
/// another user (not root) owns, could be rewritten with a key and a
/// signature of their own.
pub fn file_closed(path: &std::path::Path) -> Result<(), Failure> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(path).map_err(|e| fail(Class::Config, format!("{}: {e}", path.display())))?;
    // SAFETY: getuid has no failure.
    let me = unsafe { libc::getuid() };
    if m.mode() & 0o022 != 0 {
        return Err(fail(
            Class::Config,
            format!("{}: the group or others can write it: set mode 0644 or 0600", path.display()),
        ));
    }
    if m.uid() != me && m.uid() != 0 {
        return Err(fail(
            Class::Config,
            format!("{}: owned by uid {}, not by this user or root", path.display(), m.uid()),
        ));
    }
    Ok(())
}

/// The state directory holds the unit directories that a kind of podvm
/// mounts with `-v`, which takes no ':' (T-476): `serve` refuses such a
/// path before the first job, not at the start of each one.
pub fn state_path(cfg: &podrunner_slots::config::Config, state: &std::path::Path) -> Result<(), crate::serve::Failure> {
    if !cfg.unit.keys().any(|k| matches!(k.as_str(), "podvm" | "microvm" | "incus")) {
        return Ok(());
    }
    podrunner_unit::mounts::host_path(state, podrunner_unit::UnitCode::UnitStartFailed)
        .map_err(|e| crate::serve::fail(pod_core::failure::Class::Config, format!("--state: {}", e.detail)))
}

#[cfg(test)]
mod tests {
    /// T-629: a kind that only another host runs gives 75 (a later start
    /// can work), a file that no kind serves gives 78, and with no kind
    /// away the file is served.
    #[test]
    fn a_kind_that_this_host_lacks_is_temporary_and_a_kind_that_none_has_is_not() {
        let file = |units: &str| {
            podrunner_slots::config::Config::parse(&format!(
                "host = \"h\"\nallow_fake = true\n[[source]]\nname = \"s\"\nkind = \"fake\"\ntier = \"local\"\n\
                 arch = \"{}\"\n{units}[pin]\n",
                std::env::consts::ARCH
            ))
            .unwrap()
        };
        let away = || vec![("microvm".to_string(), "/dev/kvm: no".to_string())];
        let only = file("[unit.microvm]\nslots = 1\nbinary = \"/b\"\nimages = \"/\"\n");
        assert!(super::verdict(&only, Vec::new()).is_ok());
        let f = super::verdict(&only, away()).unwrap_err();
        assert_eq!(f.class.exit_code(), 75, "{}", f.detail);
        assert!(f.detail.contains("no-tier-served") && f.detail.contains("[unit.microvm] not here: /dev/kvm: no"));
        // The fake kind serves it: nothing to wait for.
        let both = file("[unit.fake]\nslots = 1\n[unit.microvm]\nslots = 1\nbinary = \"/b\"\nimages = \"/\"\n");
        assert!(super::verdict(&both, away()).is_ok());
        // No kind at all: 78, with or without a kind away.
        let none = file("");
        for a in [Vec::new(), away()] {
            assert_eq!(super::verdict(&none, a).unwrap_err().class.exit_code(), 78);
        }
    }

    /// T-635: a trusted source that a podvm kind does not serve names the
    /// key of the kind that is missing: `confine` for the floor, `podnet`
    /// for the hold (with the flag of `config init`).
    #[test]
    fn a_podvm_kind_that_serves_no_source_names_its_missing_key() {
        let file = |keys: &str| {
            podrunner_slots::config::Config::parse(&format!(
                "host = \"h\"\nallow_fake = true\n[[source]]\nname = \"s\"\nkind = \"fake\"\ntier = \"trusted\"\n\
                 arch = \"{}\"\n[unit.podvm]\nslots = 1\nbinary = \"/b\"\nimages = \"/\"\n{keys}[pin]\n",
                std::env::consts::ARCH
            ))
            .unwrap()
        };
        let why = |keys: &str| super::verdict(&file(keys), Vec::new()).map_err(|f| f.detail);
        let both = why("").unwrap_err();
        assert!(both.contains("[unit.podvm] has no confine = true"), "{both}");
        assert!(both.contains("[unit.podvm] has no podnet") && both.contains("--podnet PATH"), "{both}");
        let hold = why("confine = true\n").unwrap_err();
        assert!(hold.contains("[unit.podvm] has no podnet") && !hold.contains("confine = true"), "{hold}");
        let floor = why("podnet = \"/p\"\n").unwrap_err();
        assert!(floor.contains("has no confine = true") && !floor.contains("podnet"), "{floor}");
        assert!(why("confine = true\npodnet = \"/p\"\n").is_ok());
    }

    /// T-712: a kind that the self-test of `serve` lowers below the floor
    /// of the source: the line of `serve` names the source, the kind, the
    /// cause and the claim that the start lowered.
    #[test]
    fn serve_names_each_kind_that_its_start_lowers() {
        let file = |tier: &str, unit: &str| {
            podrunner_slots::config::Config::parse(&format!(
                "host = \"h\"\nallow_fake = true\n[[source]]\nname = \"s\"\nkind = \"fake\"\ntier = \"{tier}\"\n\
                 arch = \"{}\"\n[unit.fake]\nslots = 1\n{unit}[pin]\n",
                std::env::consts::ARCH
            ))
            .unwrap()
        };
        let lowered = |cfg: &podrunner_slots::config::Config| {
            let state = crate::scratch::Scratch::new("t712");
            let slots = crate::serve::build(cfg.clone(), &state).unwrap_or_else(|f| panic!("{}", f.detail));
            super::lowered(cfg, &slots)
        };
        let guest = file("trusted", "claim = \"guest\"\n");
        // `check` counts the claim: the file is served.
        assert!(super::served(&guest, false).is_ok());
        let line = lowered(&guest).expect("the self-test lowers the kind: no tier is served");
        assert!(line.starts_with("no-tier-served: no unit kind meets the floor and the egress hold"), "{line}");
        assert!(line.contains("; [[source]] s: fake: boundary None below Confined (guest-self-test-failed)"), "{line}");
        assert!(line.contains("; [unit.fake] serve lowered the boundary Guest to None"), "{line}");
        // A kind that serves its source gives no line.
        assert_eq!(lowered(&file("local", "")), None);
    }

    /// T-409: a configuration that another user can write is refused.
    #[test]
    fn a_configuration_that_others_can_write_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let f = std::env::temp_dir().join(format!("podrunner-config-mode-{}.toml", std::process::id()));
        std::fs::write(&f, "host = \"h\"\n").unwrap();
        for (mode, ok) in [(0o600, true), (0o644, true), (0o664, false), (0o646, false), (0o666, false)] {
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(mode)).unwrap();
            assert_eq!(super::file_closed(&f).is_ok(), ok, "{mode:o}");
        }
        assert!(super::file_closed(&f).unwrap_err().detail.contains("set mode 0644"));
        let _ = std::fs::remove_file(&f);
    }

    /// #287: a payload built after the clock is named; one within the skew
    /// of the clocks is not.
    #[test]
    fn a_payload_built_after_the_clock_is_named() {
        let pin = |at: u64| {
            podrunner_slots::config::Config::parse(&format!(
                "host = \"h\"\n[pin]\n[[pin.payload]]\narch = \"x86_64\"\n\
                 reference = \"r@sha256:{}\"\nbuilt_at_s = {at}\n",
                "0".repeat(64)
            ))
            .unwrap()
        };
        let now = 1_700_000_000;
        let lines = super::future_pins(&pin(now + 86_400), now);
        assert!(
            lines.len() == 1 && lines[0].contains("after the clock") && lines[0].contains("image-stale"),
            "{lines:?}"
        );
        assert!(super::future_pins(&pin(now + 60), now).is_empty());
        assert!(super::future_pins(&pin(now - 86_400), now).is_empty());
    }
}
