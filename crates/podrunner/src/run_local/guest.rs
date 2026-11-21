//! The executors of `--microvm` and `--incus` (T-123): each job in a fresh
//! guest unit with no NIC. The driver is built as `serve` builds
//! `[unit.microvm]` and `[unit.incus]`, with the same checks and the same
//! lowest version of podvm. The front of the egress socket of each unit
//! (`podrunner_local::front`) gives the broker, and the hosts of `--allow`
//! through the egress module.

use podrunner_local::exec::InUnit;
use podrunner_slots::config::UnitConfig;

use super::{Args, Class, Out, arch, fail, policy};

pub(super) fn in_guest(a: &Args, state: &std::path::Path) -> Result<InUnit, Out> {
    let (Some((microvm, binary)), Some(images), Some(image)) = (&a.guest, &a.images, &a.image) else {
        return Err(fail(Class::Usage, super::USAGE));
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
    let u = UnitConfig {
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
        mem: None,
        cpus: None,
        pids: None,
        engine: a.engine.as_ref().map(|e| e.display().to_string()),
        work_folder: None,
    };
    let built = if *microvm { crate::microvm_kind::build(&u, state) } else { crate::incus_kind::build(&u, state) };
    let (driver, _) = built.map_err(|e| fail(e.class, e.detail))?;
    let platform = if arch() == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let prepared = driver.prepare(image, platform, false).map_err(|e| fail(Class::Config, format!("{image}: {e}")))?;
    let mut exec = InUnit::new(driver, prepared);
    exec.guest = true;
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

/// The OCI layout of the mirror of the run (T-157), under the state of the
/// units, so that the end of the run removes it.
pub(super) fn mirror_dir(state: &std::path::Path) -> std::path::PathBuf {
    state.join("mirror")
}

/// The pins of the job messages. With `--engine`, each pinned image of
/// Docker Hub goes into the mirror, and a message names its manifest. When
/// the mirror fails, the engine pulls through the egress policy.
pub(super) fn mirror_pins(
    a: &Args,
    pins: &std::collections::BTreeMap<String, String>,
    w: &pod_actions_workflow::Workflow,
    state: &std::path::Path,
) -> std::collections::BTreeMap<String, String> {
    let (Some(_), Some((_, binary)), Some(store)) = (&a.engine, &a.guest, &a.images) else { return pins.clone() };
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
