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

    /// T-689: a microvm pin must leave the job `JOB_RESERVE` of root
    /// beside the boot of its image. At 4G, a pin at the boot bound of
    /// podvm boots, but its job gets less than 2 GiB: refused. At 8G (the
    /// default) the boot bound is the lesser one and stays.
    #[test]
    fn a_microvm_pin_leaves_space_for_its_job() {
        use crate::microvm_kind::{JOB_RESERVE, job_space, largest_image, mem_needed, pins_fit};
        const GIB: u64 = 1 << 30;
        let arch = std::env::consts::ARCH;
        let fit = |mem: &str, size: u64| {
            let u: podrunner_slots::config::UnitConfig = toml::from_str(&format!("slots = 1\n{mem}")).unwrap();
            let pins: Pins = toml::from_str(&format!(
                "[[payload]]\narch = \"{arch}\"\nreference = \"r@sha256:00\"\nbuilt_at_s = 1\nunpacked_bytes = {size}\n"
            ))
            .unwrap();
            pins_fit(&u, &pins).map_err(|f| f.detail)
        };
        let old = largest_image(4 * GIB);
        assert_eq!(old, 1_156_337_348);
        let e = fit("mem = \"4G\"", old).unwrap_err();
        assert!(
            e.contains("unpacks to 1.2 GB, and a guest of 4G (4.3 GB) keeps 1969 MiB for a job, less than 2048 MiB"),
            "{e}"
        );
        assert!(e.contains("fix: set mem = \"5G\" or more, or pin a smaller image"), "{e}");
        // The largest pin of 4G leaves the job its reserve, one byte more does not.
        let top = 3 * GIB - JOB_RESERVE;
        assert_eq!(job_space(4 * GIB, top), JOB_RESERVE);
        assert!(fit("mem = \"4G\"", top).is_ok());
        assert!(fit("mem = \"4G\"", top + 1).is_err());
        // The fix holds: the memory that it names takes the pin.
        assert!(fit(&format!("mem = \"{}\"", mem_needed(old)), old).is_ok());
        assert!(fit(&format!("mem = \"{}\"", mem_needed(old) - 1), old).is_err());
        // At 8G the boot bound refuses first, with the text of T-573.
        let boot = largest_image(8 * GIB);
        assert!(job_space(8 * GIB, boot) > JOB_RESERVE);
        assert!(fit("", boot).is_ok());
        let e = fit("", boot + 1).unwrap_err();
        assert!(e.contains("more than a guest of 8G (8.6 GB) boots") && e.contains("mem = \"9G\""), "{e}");
        // Edges: an empty image, a mem with no room for a reserve, a mem
        // that is not a size (plan refuses it), and an image at overflow.
        assert!(fit("mem = \"2G\"", 0).is_ok(), "an image of 0 bytes is no pin to hold");
        assert!(fit("mem = \"2G\"", 1).unwrap_err().contains("keeps 1535 MiB for a job"));
        assert!(fit("mem = \"lots\"", u64::MAX).is_ok());
        assert!(fit("", u64::MAX).unwrap_err().contains("fix: set mem = \"17179869184G\""), "no overflow");
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
