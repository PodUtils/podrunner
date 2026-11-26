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
pub fn plan(u: &UnitConfig, payload_engine: bool) -> Result<(), Failure> {
    // The keys of the podvm kind: this kind reads none of them (T-111).
    for (key, set) in [
        ("registries", !u.registries.is_empty()),
        ("runner_dir", u.runner_dir.is_some()),
        ("podnet", u.podnet.is_some()),
        ("confine", u.confine),
        ("root_quota", u.root_quota),
    ] {
        if set {
            return Err(fail(Class::Config, format!("[unit.microvm] {key}: the microvm kind does not read it")));
        }
    }
    // The init of the image runs the engine of the image: a second engine
    // of podrunner would start its own dockerd beside it (T-549).
    if u.init.is_some() && u.engine.is_some() {
        return Err(fail(Class::Config, "[unit.microvm] init = \"image\" runs the engine of the image: remove engine"));
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
    let lowest = if u.init.is_some() {
        podrunner_podvm::probe::LOWEST_MACHINE_INIT
    } else if u.pids.is_some() {
        podrunner_podvm::probe::LOWEST_GUEST_PIDS
    } else if u.engine.is_some() || payload_engine {
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

/// The bound of an image and the memory of a guest in podvm's rule
/// (T-573): one rule for `check` and the driver (T-655).
pub use podrunner_microvm::mem::{job_space, largest_image, mem_for};

/// The free root that a guest must keep for the files of a job after its
/// image (T-689). The root of a guest is in its memory: a job with less
/// fails with ENOSPC (T-688). A user who needs more sets a larger `mem`.
pub const JOB_RESERVE: u64 = 2 << 30;

/// The smallest memory of a guest that boots an image of `size` bytes and
/// keeps [`JOB_RESERVE`] for its job.
pub fn mem_needed(size: u64) -> u64 {
    mem_for(size).max(podrunner_microvm::mem::mem_for_space(size, JOB_RESERVE))
}

/// Why a guest of `bytes` (the text `mem`) does not hold an image of
/// `size` bytes and a job, or `None`.
pub fn refusal(mem: &str, bytes: u64, size: u64) -> Option<String> {
    let fix = mem_needed(size).div_ceil(1 << 30);
    if size > largest_image(bytes) {
        return Some(format!(
            "unpacks to {}, more than a guest of {mem} ({}) boots; the root of a guest is an uncompressed \
             initramfs, and podvm needs 13/4 times the image plus 512 MiB; fix: set mem = \"{fix}G\" or more",
            gb(size),
            gb(bytes),
        ));
    }
    let space = job_space(bytes, size);
    (space < JOB_RESERVE).then(|| {
        format!(
            "unpacks to {}, and a guest of {mem} ({}) keeps {} MiB for a job, less than {} MiB; the root of a \
             guest is in its memory, at 75 % of it; fix: set mem = \"{fix}G\" or more",
            gb(size),
            gb(bytes),
            space >> 20,
            JOB_RESERVE >> 20,
        )
    })
}

/// Each pin of this architecture fits the memory of a guest (T-573) with
/// space for its job (T-689). A guest with too little memory for its
/// initramfs resets with no output.
pub fn pins_fit(u: &UnitConfig, pins: &podrunner_slots::config::Pins) -> Result<(), Failure> {
    let mem = u.mem.as_deref().unwrap_or(DEFAULT_MEM);
    // `plan` refuses a `mem` that is not a size.
    let Ok(bytes) = podrunner_unit::limits::size_bytes(mem) else { return Ok(()) };
    // The largest image that passes both bounds: `refusal` holds each size
    // up to it.
    let bound = largest_image(bytes).min(podrunner_microvm::mem::root_cap(bytes).saturating_sub(JOB_RESERVE));
    let Some((p, size)) = crate::pin_size::over(pins, u.init, bound) else { return Ok(()) };
    let why = refusal(mem, bytes, size).unwrap_or_default();
    Err(fail(
        Class::Config,
        format!(
            "[unit.microvm] mem: the pin {} {why}, or pin a smaller image (the disk root of podvm, T-1994, \
             removes this bound; no version of podvm has it)",
            p.reference,
        ),
    ))
}

/// The driver and the record of `[unit.microvm]`. `sizes` gives the
/// unpacked bytes of each pinned digest: the boot floor of a guest whose
/// offer asks less memory than the kind (T-655).
pub fn build(
    u: &UnitConfig,
    state: &Path,
    payload_engines: std::collections::BTreeSet<String>,
    sizes: std::collections::BTreeMap<String, u64>,
) -> Result<(Arc<dyn UnitDriver>, Capability), Failure> {
    plan(u, !payload_engines.is_empty())?;
    let podvm = podrunner_podvm::Podvm { binary: u.binary.clone().unwrap_or_default().into() };
    let images = PathBuf::from(u.images.clone().unwrap_or_default());
    let unit = podrunner_microvm::MicrovmUnit::new(podvm, &state.join("microvm"), &images)
        .with_size(u.mem.as_deref().unwrap_or(DEFAULT_MEM), u.cpus.unwrap_or(2))
        .with_payload_engines(payload_engines)
        .with_payload_sizes(sizes)
        .with_image_init(u.init == Some(podrunner_slots::config::UnitInit::Image));
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
            podrunner_unit::program_file::write(
                &fake,
                format!("#!/bin/sh\necho '{{\"podvm\":\"{version}\",\"rung\":\"userland\"}}'\n"),
            )
            .unwrap();
            let e = if with_engine { format!("engine = \"{}\"\n", engine.display()) } else { String::new() };
            toml::from_str(&format!("slots = 1\nbinary = \"{}\"\nimages = \"{}\"\n{e}", fake.display(), dir.display()))
                .unwrap()
        };
        let e = super::plan(&unit("0.2.0-beta.13", true), false).unwrap_err();
        assert!(e.detail.contains("0.2.0-beta.13") && e.detail.contains("0.2.0-beta.14"), "{}", e.detail);
        assert!(super::plan(&unit("0.2.0-beta.13", false), false).is_ok(), "no engine: beta.10 is enough");
        assert!(super::plan(&unit("0.2.0-beta.14", true), false).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-549: `init = "image"` needs the podvm whose machine tier runs the
    /// command beside the init of the image (`0.2.0-beta.37`).
    #[test]
    fn plan_takes_the_init_of_the_image_from_beta_37() {
        let dir = std::env::temp_dir().join(format!("podrunner-microvm-init-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("podvm");
        let unit = |version: &str| -> podrunner_slots::config::UnitConfig {
            podrunner_unit::program_file::write(
                &fake,
                format!("#!/bin/sh\necho '{{\"podvm\":\"{version}\",\"rung\":\"userland\"}}'\n"),
            )
            .unwrap();
            let text = format!(
                "slots = 1\nbinary = \"{}\"\nimages = \"{}\"\ninit = \"image\"\n",
                fake.display(),
                dir.display()
            );
            toml::from_str(&text).unwrap()
        };
        let e = super::plan(&unit("0.2.0-beta.36"), false).unwrap_err();
        assert!(e.detail.contains("0.2.0-beta.37"), "{}", e.detail);
        assert!(super::plan(&unit("0.2.0-beta.37"), false).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
