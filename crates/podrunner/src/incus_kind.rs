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

use crate::pin_size::gb;
use crate::serve::{Failure, fail};

/// Check `[unit.incus]`: the podvm binary, its version (`podvm probe
/// --json`, which reads no state) and the store of the images. `mem`,
/// `cpus` and `pids` are the limits of each instance (T-162).
pub fn plan(u: &UnitConfig) -> Result<(), Failure> {
    // The keys of the podvm kind: this kind reads none of them (T-111).
    for (key, set) in [
        ("registries", !u.registries.is_empty()),
        ("runner_dir", u.runner_dir.is_some()),
        ("podnet", u.podnet.is_some()),
        ("confine", u.confine),
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
    let limits = podrunner_slots::limits::limits_of(u).map_err(|e| fail(Class::Config, format!("[unit.incus] {e}")))?;
    // The init of the image runs the engine of the image: a second engine
    // of podrunner would start its own dockerd beside it (T-543).
    let init_image = u.init == Some(podrunner_slots::config::UnitInit::Image);
    if init_image && u.engine.is_some() {
        return Err(fail(Class::Config, "[unit.incus] init = \"image\" runs the engine of the image: remove engine"));
    }
    // An engine of each instance (T-485), as for the microvm kind.
    if let Some(e) = &u.engine {
        if !Path::new(e).join("dockerd").is_file() {
            return Err(fail(Class::Config, "[unit.incus] engine: no dockerd in the directory"));
        }
        // A host path of a `-v` of podvm takes no ':' (T-476).
        podrunner_unit::mounts::host_path(Path::new(e), podrunner_unit::UnitCode::UnitStartFailed)
            .map_err(|e| fail(Class::Config, format!("[unit.incus] engine: {}", e.detail)))?;
    }
    let lowest = if init_image {
        podrunner_podvm::probe::LOWEST_INCUS_INIT
    } else if u.engine.is_some() {
        podrunner_podvm::probe::LOWEST_ENGINE_INCUS
    } else if limits.is_empty() {
        podrunner_podvm::probe::LOWEST_INCUS
    } else {
        podrunner_podvm::probe::LOWEST_INCUS_LIMITS
    };
    // The version of podvm last, as `serve` asks it (T-116).
    podrunner_podvm::Podvm { binary: binary.into() }
        .require(lowest)
        .map_err(|e| fail(Class::Unsupported, format!("[unit.incus] binary: {e}")))?;
    Ok(())
}

/// The host disk of the incus kind for each byte of an unpacked image, in
/// tenths: the extracted tree plus about 2.4 times it in incusd (the
/// measure of podvm, COORD-203).
pub const HOST_DISK_TENTHS: u64 = 34;

/// Each pin of this architecture fits the root disk of an instance (T-551):
/// with `root_quota`, `disk_max_bytes` is the size of each root, and an
/// image larger than it fails each job at its start.
pub fn pins_fit(u: &UnitConfig, pins: &podrunner_slots::config::Pins) -> Result<(), Failure> {
    let (true, Some(bound)) = (u.root_quota, u.disk_max_bytes) else { return Ok(()) };
    let Some((p, size)) = crate::pin_size::over(pins, u.init, bound) else { return Ok(()) };
    Err(fail(
        Class::Config,
        format!(
            "[unit.incus] disk_max_bytes: the pin {} unpacks to {}, more than the root disk of an instance ({}); \
             fix: set disk_max_bytes above {} with room for the writes of a job, or pin a smaller image; \
             the host needs the extracted tree plus about 2.4 times it in incusd, about {} of disk (podvm's measure)",
            p.reference,
            gb(size),
            gb(bound),
            gb(size),
            gb(size.saturating_mul(HOST_DISK_TENTHS) / 10),
        ),
    ))
}

/// The driver and the record of `[unit.incus]`.
pub fn build(u: &UnitConfig, state: &Path) -> Result<(Arc<dyn UnitDriver>, Capability), Failure> {
    plan(u)?;
    let podvm = podrunner_podvm::Podvm { binary: u.binary.clone().unwrap_or_default().into() };
    let images = PathBuf::from(u.images.clone().unwrap_or_default());
    let mut limits =
        podrunner_slots::limits::limits_of(u).map_err(|e| fail(Class::Config, format!("[unit.incus] {e}")))?;
    limits.disk_bytes = u.disk_max_bytes.filter(|_| u.root_quota);
    let mut unit = podrunner_incus::IncusUnit::new(podvm, &state.join("incus"), &images)
        .with_limits(limits)
        .with_root_quota(u.root_quota)
        .with_image_init(u.init == Some(podrunner_slots::config::UnitInit::Image));
    if let Some(e) = &u.engine {
        unit = unit.with_engine(Path::new(e));
    }
    let cap = unit.probe().map_err(|e| fail(Class::Unsupported, e.to_string()))?;
    Ok((Arc::new(unit), cap))
}
