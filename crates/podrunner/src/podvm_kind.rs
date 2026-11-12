//! The podvm unit kind of `serve`: the driver of `[unit.podvm]`, its hold,
//! and the self-tests of its tree kill and of its confine layer on this
//! host, before the record claims them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pod_core::failure::Class;
use podrunner_schema::Capability;
use podrunner_slots::config::{Config, UnitConfig};
use podrunner_unit::UnitDriver;

use crate::serve::{Failure, fail};

/// The driver and the record of `[unit.podvm]`.
pub fn build(u: &UnitConfig, cfg: &Config, state: &Path) -> Result<(Arc<dyn UnitDriver>, Capability), Failure> {
    let need = |v: &Option<String>, key: &str| {
        v.clone().ok_or_else(|| fail(Class::Config, format!("[unit.podvm] needs {key}")))
    };
    let binary = need(&u.binary, "binary")?;
    podrunner_slots::config::check_driver(Path::new(&binary))
        .map_err(|e| fail(Class::Config, format!("[unit.podvm] binary: {e}")))?;
    let podvm = podrunner_podvm::Podvm { binary: binary.into() };
    let images = PathBuf::from(need(&u.images, "images")?);
    let mut unit = podrunner_podvm::PodvmUnit::new(podvm, &state.join("podvm"), &images)
        .map_err(|e| fail(Class::Unsupported, format!("[unit.podvm]: {e}")))?
        .with_runner_dir(u.runner_dir.as_deref().unwrap_or("/home/runner"));
    if let Some(podnet) = &u.podnet {
        podrunner_slots::config::check_driver(Path::new(podnet))
            .map_err(|e| fail(Class::Config, format!("[unit.podvm] podnet: {e}")))?;
        unit = unit
            .with_podnet(Path::new(podnet))
            .map_err(|e| fail(Class::Unsupported, format!("[unit.podvm] podnet: {e}")))?;
    }
    if u.confine {
        unit = unit.with_confine();
    }
    // The tree kill is verified on this host, with the pinned
    // payload, before a wall limit counts as enforced; the
    // confine layer before the kind says `confined`.
    let arch = std::env::consts::ARCH;
    if let Some(pin) = cfg.pin.payload.iter().find(|p| p.arch == arch) {
        if u.confine {
            let id = podrunner_schema::UnitId::parse("u-999999998").expect("a unit id");
            match unit.verify_confine(&pin.reference, id) {
                Ok(true) => {}
                Ok(false) => {
                    eprintln!("podrunner: [unit.podvm]: the confine layer is not proven: the boundary stays redirect")
                }
                Err(e) => eprintln!("podrunner: [unit.podvm]: the self-test of the confine layer: {e}"),
            }
        }
        let id = podrunner_schema::UnitId::parse("u-999999999").expect("a unit id");
        match unit.verify_tree_kill(&pin.reference, id) {
            Ok(true) => {}
            Ok(false) => eprintln!("podrunner: [unit.podvm]: the tree kill is not proven: tree_kill stays unverified"),
            Err(e) => eprintln!("podrunner: [unit.podvm]: the self-test of the tree kill: {e}"),
        }
    }
    let cap = unit.probe().map_err(|e| fail(Class::Unsupported, e.to_string()))?;
    Ok((Arc::new(unit), cap))
}
