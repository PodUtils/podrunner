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
    /// The mirror of the images of the run (T-157): one for each run, also
    /// with `--parallel`, whose slots each have a directory of their own.
    pub mirror: PathBuf,
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

/// `--export DIR` and `--log-dir DIR` can take files: each is made, and a
/// probe file is written and removed, before the first job (T-313, T-322).
fn export_dir(a: &Args) -> Result<(), Out> {
    for (flag, dir) in [("--export", &a.export), ("--log-dir", &a.log_dir)] {
        let Some(dir) = dir else { continue };
        let bad = |e: std::io::Error| fail(Class::Config, format!("{flag} {}: {e}", dir.display()));
        std::fs::create_dir_all(dir).map_err(bad)?;
        let probe = dir.join(format!(".podrunner-probe-{}", std::process::id()));
        std::fs::write(&probe, b"").map_err(bad)?;
        std::fs::remove_file(&probe).map_err(bad)?;
    }
    Ok(())
}

fn host(a: &Args, c: &Common) -> Result<Box<dyn Executor>, Out> {
    export_dir(a)?;
    let dir = a.runner_dir.clone().ok_or_else(|| fail(Class::Usage, "a host route needs --runner-dir DIR"))?;
    let mut e = RunnerDir::new(dir).map_err(|e| fail(Class::Config, e))?;
    e.actions = c.actions.clone();
    e.pins = c.pins.clone();
    e.artifacts = Some(c.artifacts.clone());
    e.cache = c.cache.clone();
    e.oidc = Some(c.oidc.clone());
    e.checkout = c.checkout.clone();
    e.export = a.export.clone();
    e.quiet = a.quiet || a.json;
    Ok(Box::new(e))
}

fn unit(a: &Args, c: &Common, image: &str, state: &Path) -> Result<Box<dyn Executor>, Out> {
    export_dir(a)?;
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
    e.export = a.export.clone();
    e.toolcache = a.toolcache.clone();
    if a.engine.is_some() && c.mirror.is_dir() {
        e.mirror = Some(Arc::new(podrunner_local::mirror::Mirror::new(c.mirror.clone())));
    }
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
    // With no --platform too, the dispatch refuses a label of Windows, of
    // macOS or of another architecture by name (T-230).
    if a.platforms.is_empty() {
        let default = default.ok_or_else(|| fail(Class::Usage, super::USAGE))?;
        return Ok(Box::new(Dispatch::new(Vec::new(), Some(default))));
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
    // A binary that is not there, by its flag (T-339).
    if !binary.is_file() {
        return Err(fail(
            Class::NotFound,
            format!("--podvm {}: not a file; fix: the path of a podvm binary", binary.display()),
        ));
    }
    let podvm = Podvm { binary: binary.clone() };
    let mut unit = PodvmUnit::new(podvm, state, images).map_err(|e| fail(Class::Config, format!("podvm: {e}")))?;
    // The tar of an image stays for the next run (T-242).
    if let Ok(dir) = super::actions::user_cache("image-tars") {
        unit = unit.with_image_cache(&dir);
    }
    let mut egress = None;
    if let Some(podnet) = &a.podnet {
        // The fake broker listens on the loopback of the host: the seccomp
        // hold lets a unit reach it; the netns hold does not (T-095).
        unit = unit.with_podnet_seccomp(podnet).map_err(|e| fail(Class::Unsupported, format!("--podnet: {e}")))?;
        let hold = unit.probe().map_err(|e| fail(Class::Unavailable, format!("probe: {e}")))?.egress_hold;
        if hold != podrunner_schema::EgressHold::Advisory {
            return Err(fail(
                Class::Unsupported,
                format!("--podnet: the hold is {hold:?}; the local mode takes only the seccomp hold (advisory) now"),
            ));
        }
        egress = Some(module(a)?);
    }
    // The vnet: the front holds the egress, and a host that refuses each
    // AF_INET bind runs the unit (T-220). With no --allow, the unit reaches
    // the broker only.
    let shim = match &a.vnet {
        Some(v) => {
            if !a.allow.is_empty() || !a.environment_allow.is_empty() {
                egress = Some(module(a)?);
            }
            Some(super::vnet::shim(v, &state.join("vnet")).map_err(|e| fail(Class::Config, format!("--vnet: {e}")))?)
        }
        None => None,
    };
    // The runner image comes from the store only: a missing one is named,
    // and no registry is asked (T-272).
    super::doctor::image_in_store(binary, images, image).map_err(|e| fail(Class::Config, e))?;
    let platform = if arch() == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let prepared = unit.prepare(image, platform, false).map_err(|e| fail(Class::Config, format!("{image}: {e}")))?;
    let mut exec = InUnit::new(std::sync::Arc::new(unit), prepared);
    exec.egress = egress;
    exec.vnet = shim;
    exec.credential_dir = Some(podrunner_podvm::CREDENTIAL_DIR.to_string());
    Ok(exec)
}

/// The egress module of a podvm run and the policy of `--allow`: the proxy
/// of the environment is its way out (T-233).
fn module(a: &Args) -> Result<(std::sync::Arc<podrunner_egress::Egress>, String), Out> {
    let up = podrunner_local::fetch::env_proxy("github.com").map_err(|e| fail(Class::Config, e))?;
    let e = podrunner_egress::Egress::host(up.clone(), up.is_some(), podrunner_egress::dns::resolver_via(up.as_ref()))
        .map_err(|e| fail(Class::Unavailable, format!("the egress module: {e}")))?
        .quiet();
    Ok((std::sync::Arc::new(e), policy(&a.allow)))
}
