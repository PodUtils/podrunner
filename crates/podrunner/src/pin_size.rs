//! The size of a pinned image against the bound of each unit kind (T-551,
//! T-573): one rule and one text for `check`, `serve`, `doctor` and
//! `config init`. A pin with no `unpacked_bytes` is not checked.

use podrunner_slots::config::{Config, Payload, Pins, UnitConfig};

use crate::serve::Failure;

/// Bytes as decimal GB with one decimal, as a host gives its disk.
pub fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

/// The first pin of this architecture whose image unpacks to more than
/// `bound`, with its size.
pub fn over(pins: &Pins, bound: u64) -> Option<(&Payload, u64)> {
    let arch = std::env::consts::ARCH;
    pins.payload.iter().filter(|p| p.arch == arch).find_map(|p| p.unpacked_bytes.filter(|s| *s > bound).map(|s| (p, s)))
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
