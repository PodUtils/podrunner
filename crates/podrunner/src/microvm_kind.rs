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

use crate::pin_size::gb;
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
        ("root_quota", u.root_quota),
        ("init", u.init.is_some()),
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
    // `mem` and the other limits, as the build reads them (#232).
    podrunner_slots::limits::limits_of(u).map_err(|e| fail(Class::Config, format!("[unit.microvm] {e}")))?;
    if let Some(e) = &u.engine
        && !Path::new(e).join("dockerd").is_file()
    {
        return Err(fail(Class::Config, "[unit.microvm] engine: no dockerd in the directory"));
    }
    // The engine is a host path of a `-v` of podvm, which takes no ':' (T-476).
    if let Some(e) = &u.engine {
        podrunner_unit::mounts::host_path(Path::new(e), podrunner_unit::UnitCode::UnitStartFailed)
            .map_err(|e| fail(Class::Config, format!("[unit.microvm] engine: {}", e.detail)))?;
    }
    if u.cpus.is_some_and(|c| c == 0 || c > 64) {
        return Err(fail(Class::Config, "[unit.microvm] cpus: 1 to 64"));
    }
    // The version of podvm last: `podvm probe --json` reads no state and
    // starts no unit, so `check` and `doctor` give the answer of `serve`
    // (T-116).
    let podvm = podrunner_podvm::Podvm { binary: binary.clone().into() };
    // `pids` needs `--pids-limit` of the machine tier (T-488).
    let lowest = if u.pids.is_some() {
        podrunner_podvm::probe::LOWEST_GUEST_PIDS
    } else if u.engine.is_some() {
        podrunner_podvm::probe::LOWEST_ENGINE
    } else {
        podrunner_podvm::probe::LOWEST_GUEST
    };
    podvm.require(lowest).map_err(|e| fail(Class::Unsupported, format!("[unit.microvm] binary: {e}")))?;
    Ok(())
}

/// The memory of a guest with no `mem`. podrunner always passes
/// `--podvm-mem`: placement needs a known memory bound for each unit.
pub const DEFAULT_MEM: &str = "8G";

/// The memory of a guest beside its initramfs, in podvm's rule (COORD-219).
const GUEST_BASE: u64 = 512 << 20;

/// The largest image that a guest of `mem` bytes boots (T-573). podvm's
/// initrd is an uncompressed `newc` cpio: packed and unpacked are each about
/// the image, and the guest holds both during the unpack. podvm's rule is
/// `mem = 3 × N + 512 MiB` (COORD-219, `machine/guest/mem.rs` of podvm).
pub fn largest_image(mem: u64) -> u64 {
    mem.saturating_sub(GUEST_BASE) / 3
}

/// The memory of a guest that boots an image of `size` bytes.
pub fn mem_for(size: u64) -> u64 {
    size.saturating_mul(3).saturating_add(GUEST_BASE)
}

/// Each pin of this architecture fits the memory of a guest (T-573). A guest
/// with too little memory for its initramfs resets with no output.
pub fn pins_fit(u: &UnitConfig, pins: &podrunner_slots::config::Pins) -> Result<(), Failure> {
    let mem = u.mem.as_deref().unwrap_or(DEFAULT_MEM);
    // `plan` refuses a `mem` that is not a size.
    let Ok(bytes) = podrunner_unit::limits::size_bytes(mem) else { return Ok(()) };
    let Some((p, size)) = crate::pin_size::over(pins, largest_image(bytes)) else { return Ok(()) };
    Err(fail(
        Class::Config,
        format!(
            "[unit.microvm] mem: the pin {} unpacks to {}, more than a guest of {mem} ({}) boots; \
             the root of a guest is an uncompressed initramfs, and podvm needs 3 times the image plus 512 MiB; \
             fix: set mem = \"{}G\" or more, or pin a smaller image (the disk root of podvm, T-1994, \
             removes this bound; no version of podvm has it)",
            p.reference,
            gb(size),
            gb(bytes),
            mem_for(size).div_ceil(1 << 30),
        ),
    ))
}

/// The driver and the record of `[unit.microvm]`.
pub fn build(u: &UnitConfig, state: &Path) -> Result<(Arc<dyn UnitDriver>, Capability), Failure> {
    plan(u)?;
    let podvm = podrunner_podvm::Podvm { binary: u.binary.clone().unwrap_or_default().into() };
    let images = PathBuf::from(u.images.clone().unwrap_or_default());
    let unit = podrunner_microvm::MicrovmUnit::new(podvm, &state.join("microvm"), &images)
        .with_size(u.mem.as_deref().unwrap_or(DEFAULT_MEM), u.cpus.unwrap_or(2));
    let unit = match &u.engine {
        Some(e) => unit.with_engine(Path::new(e)),
        None => unit,
    };
    let unit = match u.pids {
        Some(n) => unit.with_pids(n),
        None => unit,
    };
    let cap = unit.probe().map_err(|e| fail(Class::Unsupported, e.to_string()))?;
    Ok((Arc::new(unit), cap))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    /// T-573: podvm's measure (COORD-219): an image of 1416 MiB resets a
    /// guest of 4G and boots at 5G. The bound is `(mem - 512 MiB) / 3`.
    #[test]
    fn the_largest_image_follows_the_measure_of_podvm() {
        const GIB: u64 = 1 << 30;
        let image = 1416 << 20;
        assert!(image > super::largest_image(4 * GIB));
        assert!(image <= super::largest_image(5 * GIB));
        assert_eq!(super::mem_for(image).div_ceil(GIB), 5);
        assert_eq!(super::largest_image(8 * GIB), 2560 << 20);
        assert_eq!(super::largest_image(1 << 20), 0);
    }

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
