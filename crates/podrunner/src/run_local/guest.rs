//! The executors of `--microvm` and `--incus` (T-123): each job in a fresh
//! guest unit with no NIC. The driver is built as `serve` builds
//! `[unit.microvm]` and `[unit.incus]`, with the same checks and the same
//! lowest version of podvm. The front of the egress socket of each unit
//! (`podrunner_local::front`) gives the broker, and the hosts of `--allow`
//! through the egress module.

use podrunner_local::exec::InUnit;
use podrunner_slots::config::UnitConfig;

use super::{Args, Class, Out, arch, fail, policy};

/// The bytes of `--mem SIZE` (T-690): `--microvm` only, a size as the
/// `mem` of `[unit.microvm]`, and room for the boot and the reserve of a job
/// with an image of 0 bytes. The unpacked size of the image is not known
/// here (podvm gives the stored size), so a larger image is not checked.
pub(super) fn mem_of(m: &str, microvm: bool) -> Result<u64, String> {
    if !microvm {
        return Err("--mem needs --microvm".into());
    }
    let bytes = podrunner_unit::limits::size_bytes(m).map_err(|e| format!("--mem {e}"))?;
    let (space, reserve) = (crate::microvm_kind::job_space(bytes, 0), crate::microvm_kind::JOB_RESERVE);
    match space >= reserve {
        true => Ok(bytes),
        false => Err(format!(
            "--mem {m}: a guest of {m} keeps {} MiB of root for a job, less than {} MiB with no image; \
             fix: --mem {}G or more, and more for the image",
            space >> 20,
            reserve >> 20,
            crate::microvm_kind::mem_needed(0).div_ceil(1 << 30)
        )),
    }
}

/// The text of the `mem` check of `doctor --run` (T-690).
pub(super) fn mem_line(a: &Args) -> Result<String, String> {
    let m = a.mem.as_deref().unwrap_or(crate::microvm_kind::DEFAULT_MEM);
    let bytes = mem_of(m, true)?;
    let given = if a.mem.is_some() { "--mem" } else { "the default" };
    Ok(format!(
        "{m} ({given}): a job gets about {} of root less the unpacked image",
        crate::pin_size::gb(podrunner_microvm::mem::root_cap(bytes))
    ))
}

pub(super) fn in_guest(a: &Args, state: &std::path::Path) -> Result<InUnit, Out> {
    let (Some((microvm, binary)), Some(images), Some(image)) = (&a.guest, &a.images, &a.image) else {
        return Err(fail(Class::Usage, super::usage()));
    };
    if !binary.is_file() {
        let flag = if *microvm { "--microvm" } else { "--incus" };
        return Err(fail(
            Class::NotFound,
            format!("{flag} {}: not a file; fix: the path of a podvm binary", binary.display()),
        ));
    }
    // The checks of --podvm before a guest starts: the image is in the store
    // (no pull), and the machine tier has /dev/kvm (T-297).
    if *microvm {
        kvm().map_err(|e| fail(Class::Unavailable, e))?;
        super::doctor::image_in_store(binary, images, image).map_err(|e| fail(Class::Config, e))?;
    }
    let u = unit(a, binary, images);
    let payload_engines = a.engine.is_none().then(|| payload_engine(a, image)).flatten();
    let payload_engines = payload_engines.into_iter().collect();
    let built = if *microvm {
        crate::microvm_kind::build(&u, state, payload_engines, Default::default())
    } else {
        crate::incus_kind::build(&u, state, payload_engines)
    };
    let (driver, cap) = built.map_err(|e| fail(e.class, e.detail))?;
    if !*microvm {
        crate::incus_kind::warn_engine("podrunner", &cap);
    }
    let platform = if arch() == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let prepared = driver.prepare(image, platform, false).map_err(|e| fail(Class::Config, format!("{image}: {e}")))?;
    let mut exec = InUnit::new(driver, prepared);
    exec.guest = true;
    exec.guest_engine = cap.container == podrunner_schema::ContainerRoute::Engine;
    if !a.allow.is_empty() || !a.environment_allow.is_empty() {
        // The proxy of the environment is the way out of the module (T-233).
        let up = podrunner_local::fetch::env_proxy("github.com").map_err(|e| fail(Class::Config, e))?;
        let e =
            podrunner_egress::Egress::host(up.clone(), up.is_some(), podrunner_egress::dns::resolver_via(up.as_ref()))
                .map_err(|e| fail(Class::Unavailable, format!("the egress module: {e}")))?
                .quiet();
        exec.egress = Some((std::sync::Arc::new(e), policy(&a.allow)));
    }
    Ok(exec)
}

/// The `[unit.KIND]` of the guest of a run: `serve` builds the kind from it.
fn unit(a: &Args, binary: &std::path::Path, images: &std::path::Path) -> UnitConfig {
    UnitConfig {
        slots: 1,
        command: vec!["./run.sh".into()],
        binary: Some(binary.display().to_string()),
        images: Some(images.display().to_string()),
        registries: Vec::new(),
        runner_dir: None,
        podnet: None,
        disk_max_bytes: None,
        root_quota: false,
        init: None,
        claim: None,
        confine: false,
        // `parse` takes `--mem` with `--microvm` only.
        mem: a.mem.clone(),
        cpus: None,
        pids: None,
        engine: a.engine.as_ref().map(|e| e.display().to_string()),
        work_folder: None,
        // `run` makes no warm unit: one job, one unit.
        warm: None,
        warm_max_age_s: None,
    }
}

fn payload_engine(a: &Args, image: &str) -> Option<String> {
    let (_, binary) = a.guest.as_ref()?;
    let store = a.images.as_deref()?;
    podrunner_podvm::Podvm { binary: binary.clone() }.payload_engine_digest(store, image)
}

pub(super) fn has_engine(a: &Args) -> bool {
    a.engine.is_some()
        || a.image
            .iter()
            .chain(a.platforms.iter().map(|(_, image)| image))
            .any(|image| payload_engine(a, image).is_some())
}

/// The OCI layout of the mirror of the run (T-157), under the state of the
/// units, so that the end of the run removes it.
pub(super) fn mirror_dir(state: &std::path::Path) -> std::path::PathBuf {
    state.join("mirror")
}

/// The pins of the job messages. With a guest engine, each pinned image of
/// Docker Hub goes into the mirror, and a message names its manifest.
pub(super) fn mirror_pins(
    a: &Args,
    pins: &std::collections::BTreeMap<String, String>,
    w: &pod_actions_workflow::Workflow,
    state: &std::path::Path,
) -> std::collections::BTreeMap<String, String> {
    let (Some((_, binary)), Some(store)) = (&a.guest, &a.images) else { return pins.clone() };
    if !has_engine(a) {
        return pins.clone();
    }
    let platform = if arch() == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let pull =
        podrunner_local::images::PodvmPull { binary: binary.clone(), store: store.clone(), platform: platform.into() };
    match podrunner_local::mirror::fill(&pull, pins, &podrunner_local::mirror::by_digest(w), &mirror_dir(state)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("podrunner: the mirror of the images: {e}; the engine pulls them through the egress policy");
            pins.clone()
        }
    }
}

/// The checks of a run of `--microvm` before the pin and the pull of its
/// images: `/dev/kvm` (T-297, T-327). A dry run starts no guest.
pub(super) fn early(a: &Args) -> Result<(), Out> {
    match !a.dry_run && a.guest.as_ref().is_some_and(|g| g.0) {
        true => kvm().map_err(|e| fail(Class::Unavailable, e)),
        false => Ok(()),
    }
}

/// Whether podvm gives container hooks on this host (its probe, T-311). A
/// probe that fails says nothing: the start of a job names its cause.
pub(super) fn hooks(a: &Args) -> bool {
    let probe = |b: &std::path::PathBuf| podrunner_podvm::Podvm { binary: b.clone() }.probe();
    !a.podvm.as_ref().is_some_and(|b| probe(b).is_ok_and(|c| c.container != podrunner_schema::ContainerRoute::Hooks))
}

/// `/dev/kvm` opens for reading and writing: the machine tier needs it.
pub(super) fn kvm() -> Result<(), String> {
    std::fs::OpenOptions::new().read(true).write(true).open("/dev/kvm").map(|_| ()).map_err(|e| {
        format!("--microvm: /dev/kvm: {e}; fix: a host with KVM, and a user that can open /dev/kvm (the group kvm), or --podvm")
    })
}

#[cfg(test)]
#[path = "guest_tests.rs"]
mod tests;
