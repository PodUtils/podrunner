//! The size of a pinned image against the bound of each unit kind (T-551,
//! T-573): one rule and one text for `check`, `serve`, `doctor` and
//! `config init`. A pin with no `unpacked_bytes` is not checked.

use podrunner_slots::config::{Config, Payload, Pins, UnitConfig, UnitInit};

use crate::serve::Failure;

/// Bytes as decimal GB with one decimal, as a host gives its disk.
pub fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

/// The first pin of this architecture that a kind of `init` runs (T-549)
/// whose image unpacks to more than `bound`, with its size.
pub fn over(pins: &Pins, init: Option<UnitInit>, bound: u64) -> Option<(&Payload, u64)> {
    let arch = std::env::consts::ARCH;
    pins.payload
        .iter()
        .filter(|p| p.arch == arch && p.init == init)
        .find_map(|p| p.unpacked_bytes.filter(|s| *s > bound).map(|s| (p, s)))
}

/// The unpacked bytes of each pinned digest of this architecture: the
/// microvm kind gives a guest the boot floor of its image (T-655).
pub fn sizes(pins: &Pins) -> std::collections::BTreeMap<String, u64> {
    let arch = std::env::consts::ARCH;
    pins.payload
        .iter()
        .filter(|p| p.arch == arch)
        .filter_map(|p| {
            let (_, digest) = podrunner_unit::parse_pinned(&p.reference).ok()?;
            Some((digest.to_string(), p.unpacked_bytes?))
        })
        .collect()
}

/// Each pin of this architecture fits the unit kind `kind`.
pub fn kind_fits(kind: &str, u: &UnitConfig, pins: &Pins) -> Result<(), Failure> {
    match kind {
        "incus" => crate::incus_kind::pins_fit(u, pins),
        "microvm" => crate::microvm_kind::pins_fit(u, pins),
        _ => Ok(()),
    }
}

/// Each pin of this architecture fits each unit kind of `cfg`.
pub fn fits(cfg: &Config) -> Result<(), Failure> {
    cfg.unit.iter().try_for_each(|(kind, u)| kind_fits(kind, u, &cfg.pin))
}

#[cfg(test)]
mod tests {
    use podrunner_slots::config::{Pins, UnitInit};

    /// T-549: a kind is checked only against the pins that placement sends
    /// it: a large image with its own init is not a pin of a kind with no
    /// init.
    #[test]
    fn a_kind_is_checked_against_the_pins_of_its_init() {
        let arch = std::env::consts::ARCH;
        let pins: Pins = toml::from_str(&format!(
            "[[payload]]\narch = \"{arch}\"\nreference = \"r@sha256:00\"\nbuilt_at_s = 1\nunpacked_bytes = 2000\n\
             [[payload]]\narch = \"{arch}\"\nreference = \"h@sha256:00\"\nbuilt_at_s = 1\nunpacked_bytes = 9000\n\
             init = \"image\"\n"
        ))
        .unwrap();
        let over = |init, bound| super::over(&pins, init, bound).map(|(p, s)| (p.reference.clone(), s));
        assert_eq!(over(None, 5000), None, "the image with an init does not count");
        assert_eq!(over(None, 1000), Some(("r@sha256:00".into(), 2000)));
        assert_eq!(over(Some(UnitInit::Image), 5000), Some(("h@sha256:00".into(), 9000)));
    }

    /// T-655: the size of each pinned digest of this architecture; a pin
    /// with no `unpacked_bytes` or of another architecture has none.
    #[test]
    fn the_sizes_are_those_of_the_pins_of_this_architecture() {
        let arch = std::env::consts::ARCH;
        let (h01, h02, h03) = ("1".repeat(64), "2".repeat(64), "3".repeat(64));
        let pins: Pins = toml::from_str(&format!(
            "[[payload]]\narch = \"{arch}\"\nreference = \"r@sha256:{h01}\"\nbuilt_at_s = 1\nunpacked_bytes = 2000\n\
             [[payload]]\narch = \"{arch}\"\nreference = \"s@sha256:{h02}\"\nbuilt_at_s = 1\n\
             [[payload]]\narch = \"other\"\nreference = \"t@sha256:{h03}\"\nbuilt_at_s = 1\nunpacked_bytes = 9\n"
        ))
        .unwrap();
        assert_eq!(super::sizes(&pins), [(format!("sha256:{h01}"), 2000)].into());
    }
}
