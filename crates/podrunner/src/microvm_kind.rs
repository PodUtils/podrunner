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

/// Check `[unit.microvm]`: the podvm binary, its version (`podvm probe
/// --json`, which reads no state), the store of the images and the engine
/// directory.
pub fn plan(u: &UnitConfig) -> Result<(), Failure> {
    // The keys of the podvm kind: this kind reads none of them (T-111).
    for (key, set) in [
        ("registries", !u.registries.is_empty()),
        ("runner_dir", u.runner_dir.is_some()),
        ("podnet", u.podnet.is_some()),
        ("confine", u.confine),
    ] {
        if set {
            return Err(fail(Class::Config, format!("[unit.microvm] {key}: the microvm kind does not read it")));
        }
    }
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
    if let Some(e) = &u.engine
        && !Path::new(e).join("dockerd").is_file()
    {
        return Err(fail(Class::Config, "[unit.microvm] engine: no dockerd in the directory"));
    }
    if u.cpus.is_some_and(|c| c == 0 || c > 64) {
        return Err(fail(Class::Config, "[unit.microvm] cpus: 1 to 64"));
    }
    // The version of podvm last: `podvm probe --json` reads no state and
    // starts no unit, so `check` and `doctor` give the answer of `serve`
    // (T-116).
    let podvm = podrunner_podvm::Podvm { binary: binary.clone().into() };
    let lowest =
        if u.engine.is_some() { podrunner_podvm::probe::LOWEST_ENGINE } else { podrunner_podvm::probe::LOWEST_GUEST };
    podvm.require(lowest).map_err(|e| fail(Class::Unsupported, format!("[unit.microvm] binary: {e}")))?;
    Ok(())
}

/// The driver and the record of `[unit.microvm]`.
pub fn build(u: &UnitConfig, state: &Path) -> Result<(Arc<dyn UnitDriver>, Capability), Failure> {
    plan(u)?;
    let podvm = podrunner_podvm::Podvm { binary: u.binary.clone().unwrap_or_default().into() };
    let images = PathBuf::from(u.images.clone().unwrap_or_default());
    let unit = podrunner_microvm::MicrovmUnit::new(podvm, &state.join("microvm"), &images)
        .with_size(u.mem.as_deref().unwrap_or("4G"), u.cpus.unwrap_or(2));
    let unit = match &u.engine {
        Some(e) => unit.with_engine(Path::new(e)),
        None => unit,
    };
    let cap = unit.probe().map_err(|e| fail(Class::Unsupported, e.to_string()))?;
    Ok((Arc::new(unit), cap))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    /// T-116: `plan`, which `check` and `doctor` call, refuses a podvm below
    /// the lowest version of the kind, as `serve` does.
    #[test]
    fn plan_refuses_a_podvm_too_old_for_an_engine() {
        let dir = std::env::temp_dir().join(format!("podrunner-microvm-plan-{}", std::process::id()));
        let engine = dir.join("engine");
        std::fs::create_dir_all(&engine).unwrap();
        std::fs::write(engine.join("dockerd"), "").unwrap();
        let fake = dir.join("podvm");
        let unit = |version: &str, with_engine: bool| -> podrunner_slots::config::UnitConfig {
            std::fs::write(&fake, format!("#!/bin/sh\necho '{{\"podvm\":\"{version}\",\"rung\":\"userland\"}}'\n"))
                .unwrap();
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
            let e = if with_engine { format!("engine = \"{}\"\n", engine.display()) } else { String::new() };
            toml::from_str(&format!("slots = 1\nbinary = \"{}\"\nimages = \"{}\"\n{e}", fake.display(), dir.display()))
                .unwrap()
        };
        let e = super::plan(&unit("0.2.0-beta.13", true)).unwrap_err();
        assert!(e.detail.contains("0.2.0-beta.13") && e.detail.contains("0.2.0-beta.14"), "{}", e.detail);
        assert!(super::plan(&unit("0.2.0-beta.13", false)).is_ok(), "no engine: beta.10 is enough");
        assert!(super::plan(&unit("0.2.0-beta.14", true)).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
