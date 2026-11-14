//! The incus unit kind of `serve` (T-105, githome's G-02): the driver of
//! `[unit.incus]`, a system container of the incus tier of podvm for each
//! job. The kind claims `confined`, the floor of `trusted`: a system
//! container shares the kernel of the host, so it never serves `untrusted`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pod_core::failure::Class;
use podrunner_schema::Capability;
use podrunner_slots::config::UnitConfig;
use podrunner_unit::UnitDriver;

use crate::serve::{Failure, fail};

/// Check `[unit.incus]`: the podvm binary, its version (`podvm probe
/// --json`, which reads no state) and the store of the images. `mem` and
/// `cpus` are of the machine tier only.
pub fn plan(u: &UnitConfig) -> Result<(), Failure> {
    // The keys of the podvm kind: this kind reads none of them (T-111).
    for (key, set) in [
        ("registries", !u.registries.is_empty()),
        ("runner_dir", u.runner_dir.is_some()),
        ("podnet", u.podnet.is_some()),
        ("confine", u.confine),
        ("engine", u.engine.is_some()),
    ] {
        if set {
            return Err(fail(Class::Config, format!("[unit.incus] {key}: the incus kind does not read it")));
        }
    }
    let binary = u.binary.clone().ok_or_else(|| fail(Class::Config, "[unit.incus] needs binary"))?;
    podrunner_slots::config::check_driver(Path::new(&binary))
        .map_err(|e| fail(Class::Config, format!("[unit.incus] binary: {e}")))?;
    if !Path::new(&binary).is_file() {
        return Err(fail(Class::Config, "[unit.incus] binary: not a file"));
    }
    let images = u.images.clone().ok_or_else(|| fail(Class::Config, "[unit.incus] needs images"))?;
    if !Path::new(&images).is_dir() {
        return Err(fail(Class::Config, "[unit.incus] images: not a directory"));
    }
    if u.mem.is_some() || u.cpus.is_some() {
        return Err(fail(Class::Config, "[unit.incus] mem and cpus are of the microvm kind only"));
    }
    // The version of podvm last, as `serve` asks it (T-116).
    podrunner_podvm::Podvm { binary: binary.into() }
        .require(podrunner_podvm::probe::LOWEST_INCUS)
        .map_err(|e| fail(Class::Unsupported, format!("[unit.incus] binary: {e}")))?;
    Ok(())
}

/// The driver and the record of `[unit.incus]`.
pub fn build(u: &UnitConfig, state: &Path) -> Result<(Arc<dyn UnitDriver>, Capability), Failure> {
    plan(u)?;
    let podvm = podrunner_podvm::Podvm { binary: u.binary.clone().unwrap_or_default().into() };
    let images = PathBuf::from(u.images.clone().unwrap_or_default());
    let unit = podrunner_incus::IncusUnit::new(podvm, &state.join("incus"), &images);
    let cap = unit.probe().map_err(|e| fail(Class::Unsupported, e.to_string()))?;
    Ok((Arc::new(unit), cap))
}
