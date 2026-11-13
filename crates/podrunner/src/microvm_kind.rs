//! The microvm unit kind of `serve` (T-037): the driver of
//! `[unit.microvm]`, a guest of the machine tier of podvm for each job. The
//! kind claims `guest`; `serve` takes the claim only for a kind on
//! `guest_kinds` and after the self-test of GitHub #36.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pod_core::failure::Class;
use podrunner_schema::Capability;
use podrunner_slots::config::UnitConfig;
use podrunner_unit::UnitDriver;

use crate::serve::{Failure, fail};

/// Check `[unit.microvm]` with no side effect: the podvm binary and the
/// store of the images.
pub fn plan(u: &UnitConfig) -> Result<(), Failure> {
    let binary = u.binary.clone().ok_or_else(|| fail(Class::Config, "[unit.microvm] needs binary"))?;
    podrunner_slots::config::check_driver(Path::new(&binary))
        .map_err(|e| fail(Class::Config, format!("[unit.microvm] binary: {e}")))?;
    if !Path::new(&binary).is_file() {
        return Err(fail(Class::Config, "[unit.microvm] binary: not a file"));
    }
    let images = u.images.clone().ok_or_else(|| fail(Class::Config, "[unit.microvm] needs images"))?;
    if !Path::new(&images).is_dir() {
        return Err(fail(Class::Config, "[unit.microvm] images: not a directory"));
    }
    if u.cpus.is_some_and(|c| c == 0 || c > 64) {
        return Err(fail(Class::Config, "[unit.microvm] cpus: 1 to 64"));
    }
    Ok(())
}

/// The driver and the record of `[unit.microvm]`.
pub fn build(u: &UnitConfig, state: &Path) -> Result<(Arc<dyn UnitDriver>, Capability), Failure> {
    plan(u)?;
    let podvm = podrunner_podvm::Podvm { binary: u.binary.clone().unwrap_or_default().into() };
    podvm
        .require(podrunner_podvm::probe::LOWEST_GUEST)
        .map_err(|e| fail(Class::Unsupported, format!("[unit.microvm] binary: {e}")))?;
    let images = PathBuf::from(u.images.clone().unwrap_or_default());
    let unit = podrunner_microvm::MicrovmUnit::new(podvm, &state.join("microvm"), &images)
        .with_size(u.mem.as_deref().unwrap_or("4G"), u.cpus.unwrap_or(2));
    let cap = unit.probe().map_err(|e| fail(Class::Unsupported, e.to_string()))?;
    Ok((Arc::new(unit), cap))
}
