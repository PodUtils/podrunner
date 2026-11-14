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
    let u = UnitConfig {
        slots: 1,
        command: vec!["./run.sh".into()],
        binary: Some(binary.display().to_string()),
        images: Some(images.display().to_string()),
        registries: Vec::new(),
        runner_dir: None,
        podnet: None,
        disk_max_bytes: None,
        confine: false,
        mem: None,
        cpus: None,
        engine: a.engine.as_ref().map(|e| e.display().to_string()),
    };
    let built = if *microvm { crate::microvm_kind::build(&u, state) } else { crate::incus_kind::build(&u, state) };
    let (driver, _) = built.map_err(|e| fail(e.class, e.detail))?;
    let platform = if arch() == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let prepared = driver.prepare(image, platform, false).map_err(|e| fail(Class::Config, format!("{image}: {e}")))?;
    let mut exec = InUnit::new(driver, prepared);
    exec.guest = true;
    if !a.allow.is_empty() || !a.environment_allow.is_empty() {
        let e = podrunner_egress::Egress::host(None, true, podrunner_egress::dns::resolver(false, Vec::new()))
            .map_err(|e| fail(Class::Unavailable, format!("the egress module: {e}")))?;
        exec.egress = Some((std::sync::Arc::new(e), policy(&a.allow)));
    }
    Ok(exec)
}

/// The OCI layout of the mirror of the run (T-157), under the state of the
/// units, so that the end of the run removes it.
pub(super) fn mirror_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("podrunner-run-units-{}", std::process::id())).join("mirror")
}

/// The pins of the job messages. With `--engine`, each pinned image of
/// Docker Hub goes into the mirror, and a message names its manifest. When
/// the mirror fails, the engine pulls through the egress policy.
pub(super) fn mirror_pins(
    a: &Args,
    pins: &std::collections::BTreeMap<String, String>,
    w: &pod_actions_workflow::Workflow,
) -> std::collections::BTreeMap<String, String> {
    let (Some(_), Some((true, binary)), Some(store)) = (&a.engine, &a.guest, &a.images) else { return pins.clone() };
    let platform = if arch() == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let pull =
        podrunner_local::images::PodvmPull { binary: binary.clone(), store: store.clone(), platform: platform.into() };
    match podrunner_local::mirror::fill(&pull, pins, &podrunner_local::mirror::by_digest(w), &mirror_dir()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("podrunner: the mirror of the images: {e}; the engine pulls them through the egress policy");
            pins.clone()
        }
    }
}
