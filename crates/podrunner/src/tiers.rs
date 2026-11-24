//! The check of `check` and `serve` that a kind can serve a source (T-185).

use pod_core::failure::Class;
use podrunner_schema::{Boundary, EgressHold};
use podrunner_slots::config::Config;

use crate::serve::{Failure, fail, fake_capability};

/// A file whose sources no kind can serve is refused, as `serve` says
/// `no-tier-served`. Each kind counts at the best boundary that it can
/// claim, so a self-test of `serve` can only lower it. A kind also needs
/// the egress hold of the tier, as placement does (#280): a podvm kind
/// has a hold only with `podnet`, and then at best `enforced`.
pub fn served(cfg: &Config) -> Result<(), Failure> {
    let best = |name: &str, u: &podrunner_slots::config::UnitConfig| match name {
        "fake" => crate::serve::fake_capability_of(u).boundary,
        "podvm" if u.confine => Boundary::Confined,
        "podvm" => Boundary::Redirect,
        "incus" => Boundary::Confined,
        _ => Boundary::Guest,
    };
    let hold = |name: &str, u: &podrunner_slots::config::UnitConfig| match name {
        "fake" => crate::serve::fake_capability_of(u).egress_hold,
        "podvm" if u.podnet.is_none() => EgressHold::Unknown,
        _ => EgressHold::Enforced,
    };
    let arch = std::env::consts::ARCH;
    // A kind that this host cannot run serves nothing: `serve` drops it.
    let away = away(cfg);
    let served = cfg.source.iter().any(|s| {
        let (floor, need) = (cfg.floor(s.tier), podrunner_slots::placement::needed_hold(s.tier));
        cfg.unit.iter().filter(|(n, _)| !away.iter().any(|(a, _)| a == *n)).any(|(n, u)| {
            let kind_arch = if n == "fake" { fake_capability().arch } else { arch.to_string() };
            kind_arch == s.arch && best(n, u) >= floor && hold(n, u).effective() >= need
        })
    });
    if !cfg.source.is_empty() && !served {
        let why: String = away.iter().map(|(n, w)| format!(" ([unit.{n}] not here: {w})")).collect();
        return Err(fail(
            Class::Config,
            format!("no-tier-served: no unit kind meets the floor and the egress hold of a source on this host{why}"),
        ));
    }
    Ok(())
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
