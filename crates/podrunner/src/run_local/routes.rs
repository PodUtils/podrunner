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

use super::{Args, Class, Out, fail, guest, in_podvm};

/// The settings that each executor of the run takes.
pub(super) struct Common {
    pub actions: Option<PathBuf>,
    pub pins: std::collections::BTreeMap<String, String>,
    pub artifacts: Arc<ArtifactStore>,
    pub cache: Option<Arc<CacheStore>>,
    pub checkout: Option<PathBuf>,
}

fn host(a: &Args, c: &Common) -> Result<Box<dyn Executor>, Out> {
    let dir = a.runner_dir.clone().ok_or_else(|| fail(Class::Usage, "a host route needs --runner-dir DIR"))?;
    let mut e = RunnerDir::new(dir).map_err(|e| fail(Class::Config, e))?;
    e.actions = c.actions.clone();
    e.pins = c.pins.clone();
    e.artifacts = Some(c.artifacts.clone());
    e.cache = c.cache.clone();
    e.checkout = c.checkout.clone();
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
