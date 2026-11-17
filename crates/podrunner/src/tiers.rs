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
        "fake" => fake_capability().boundary,
        "podvm" if u.confine => Boundary::Confined,
        "podvm" => Boundary::Redirect,
        "incus" => Boundary::Confined,
        _ => Boundary::Guest,
    };
    let hold = |name: &str, u: &podrunner_slots::config::UnitConfig| match name {
        "fake" => fake_capability().egress_hold,
        "podvm" if u.podnet.is_none() => EgressHold::Unknown,
        _ => EgressHold::Enforced,
    };
    let arch = std::env::consts::ARCH;
    let served = cfg.source.iter().any(|s| {
        let (floor, need) = (cfg.floor(s.tier), podrunner_slots::placement::needed_hold(s.tier));
        cfg.unit.iter().any(|(n, u)| {
            let kind_arch = if n == "fake" { fake_capability().arch } else { arch.to_string() };
            kind_arch == s.arch && best(n, u) >= floor && hold(n, u).effective() >= need
        })
    });
    if !cfg.source.is_empty() && !served {
        return Err(fail(
            Class::Config,
            "no-tier-served: no unit kind meets the floor and the egress hold of a source on this host",
        ));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
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
