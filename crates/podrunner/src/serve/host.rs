//! The facts of this host that `serve` gives each source in `hello` at its
//! start (T-495, githome's G-17).

use podrunner_slots::Slots;
use podrunner_slots::config::Config;
use podrunner_source::HostFacts;

/// The version of podvm of the first kind that names a podvm binary.
pub(crate) fn podvm_version(cfg: &Config) -> Option<String> {
    let binary = cfg.unit.values().find_map(|u| u.binary.clone())?;
    podrunner_podvm::Podvm { binary: binary.into() }.version_text()
}

/// The facts of this host, with the kinds of `slots`.
pub(crate) fn facts(slots: &Slots, podvm: Option<String>) -> HostFacts {
    HostFacts {
        podrunner: env!("CARGO_PKG_VERSION").to_string(),
        podvm,
        podnet: None,
        arch: std::env::consts::ARCH.to_string(),
        kvm: std::path::Path::new("/dev/kvm").exists(),
        kinds: slots.kind_facts(),
    }
}
