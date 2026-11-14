//! The executor of a run (T-129): one executor, or one for each label of
//! `--platform LABEL=TARGET` and a default. TARGET `host` is the runner
//! directory; TARGET `NAME@sha256:HEX` is a unit of the kind of the run with
//! that image. Each route of a unit kind has its own state directory, so two
//! routes never share a unit id.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use podrunner_local::artifacts::ArtifactStore;
use podrunner_local::cache::CacheStore;
use podrunner_local::dispatch::Dispatch;
use podrunner_local::drive::Executor;
use podrunner_local::exec::{InUnit, RunnerDir};

use super::{Args, Class, Out, USAGE, arch, fail, guest, policy};

/// The settings that each executor of the run takes.
pub(super) struct Common {
    pub actions: Option<PathBuf>,
    pub pins: std::collections::BTreeMap<String, String>,
    pub artifacts: Arc<ArtifactStore>,
    pub cache: Option<Arc<CacheStore>>,
    pub checkout: Option<PathBuf>,
    pub oidc: Arc<podrunner_local::oidc::Issuer>,
}

/// The issuer of the id-tokens of the run (T-140): `--oidc-issuer URL`,
/// one new key; `--oidc-keys FILE` gets its public keys.
pub(super) fn issuer(a: &Args) -> Result<Arc<podrunner_local::oidc::Issuer>, String> {
    let name = a.oidc_issuer.as_deref().unwrap_or("https://oidc.podrunner.invalid");
    let issuer = podrunner_local::oidc::Issuer::new(name)?;
    if let Some(file) = &a.oidc_keys {
        std::fs::write(file, format!("{:#}\n", issuer.jwks()))
            .map_err(|e| format!("--oidc-keys {}: {e}", file.display()))?;
    }
    Ok(Arc::new(issuer))
}

fn host(a: &Args, c: &Common) -> Result<Box<dyn Executor>, Out> {
    let dir = a.runner_dir.clone().ok_or_else(|| fail(Class::Usage, "a host route needs --runner-dir DIR"))?;
    let mut e = RunnerDir::new(dir).map_err(|e| fail(Class::Config, e))?;
    e.actions = c.actions.clone();
    e.pins = c.pins.clone();
    e.artifacts = Some(c.artifacts.clone());
    e.cache = c.cache.clone();
    e.oidc = Some(c.oidc.clone());
    e.checkout = c.checkout.clone();
    e.export = a.export.clone();
    Ok(Box::new(e))
}

fn unit(a: &Args, c: &Common, image: &str, state: &Path) -> Result<Box<dyn Executor>, Out> {
    let mut a = a.clone();
    a.image = Some(image.to_string());
    let mut e: InUnit = match &a.guest {
        Some(_) => guest::in_guest(&a, state)?,
        None => in_podvm(&a, state)?,
    };
    e.actions = c.actions.clone();
    e.pins = c.pins.clone();
    e.artifacts = Some(c.artifacts.clone());
    e.cache = c.cache.clone();
    e.oidc = Some(c.oidc.clone());
    e.checkout = c.checkout.clone();
    // The egress policy of each environment (T-136).
    let mut hosts: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (name, host) in &a.environment_allow {
        hosts.entry(name.clone()).or_default().push(host.clone());
    }
    e.environment_policies = hosts.into_iter().map(|(n, h)| (n, super::policy(&h))).collect();
    Ok(Box::new(e))
}

/// The executor of the run, with the state of its units under `state`.
pub(super) fn build(a: &Args, c: &Common, state: &Path) -> Result<Box<dyn Executor>, Out> {
    let default = match (&a.image, a.runner_dir.is_some()) {
        (Some(image), _) => Some(unit(a, c, image, &state.join("default"))?),
        (None, true) => Some(host(a, c)?),
        (None, false) => None,
    };
    if a.platforms.is_empty() {
        return default.ok_or_else(|| fail(Class::Usage, super::USAGE));
    }
    let mut routes = Vec::new();
    for (i, (label, target)) in a.platforms.iter().enumerate() {
        let e = match target.as_str() {
            "host" => host(a, c)?,
            image => unit(a, c, image, &state.join(format!("route-{i}")))?,
        };
        routes.push((label.clone(), e));
    }
    Ok(Box::new(Dispatch::new(routes, default)))
}

/// The executor of `--podvm`: each job in a fresh podvm unit of the
/// runner image (T-090), with the state of the units in `state`.
fn in_podvm(a: &Args, state: &std::path::Path) -> Result<InUnit, Out> {
    use podrunner_podvm::{Podvm, PodvmUnit};
    use podrunner_unit::UnitDriver;
    let (Some(binary), Some(images), Some(image)) = (&a.podvm, &a.images, &a.image) else {
        return Err(fail(Class::Usage, USAGE));
    };
    let podvm = Podvm { binary: binary.clone() };
    let mut unit = PodvmUnit::new(podvm, state, images).map_err(|e| fail(Class::Config, format!("podvm: {e}")))?;
    let mut egress = None;
    if let Some(podnet) = &a.podnet {
        unit = unit.with_podnet(podnet).map_err(|e| fail(Class::Unsupported, format!("--podnet: {e}")))?;
        let hold = unit.probe().map_err(|e| fail(Class::Unavailable, format!("probe: {e}")))?.egress_hold;
        // The fake broker listens on the loopback of the host: the seccomp
        // hold lets a unit reach it; the netns hold does not (T-095).
        if hold != podrunner_schema::EgressHold::Advisory {
            return Err(fail(
                Class::Unsupported,
                format!("--podnet: the hold is {hold:?}; the local mode takes only the seccomp hold (advisory) now"),
            ));
        }
        let e = podrunner_egress::Egress::host(None, true, podrunner_egress::dns::resolver(false, Vec::new()))
            .map_err(|e| fail(Class::Unavailable, format!("the egress module: {e}")))?;
        egress = Some((std::sync::Arc::new(e), policy(&a.allow)));
    }
    let platform = if arch() == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let prepared = unit.prepare(image, platform, false).map_err(|e| fail(Class::Config, format!("{image}: {e}")))?;
    let mut exec = InUnit::new(std::sync::Arc::new(unit), prepared);
    exec.egress = egress;
    Ok(exec)
}
