//! The jobs with containers that the executor of a run cannot run (T-327,
//! T-330): one refusal that says what the host needs, before the pin and
//! the pull of their images. A run with `--platform` routes each job, so it
//! gets none here: each executor refuses at the start of the job.

use std::collections::BTreeMap;

use podrunner_local::message::{REFUSE_CONTAINERS, REFUSE_SERVICES};

use super::Args;

/// The images of a unit kind come into its store by `podvm pull` (T-130).
pub(super) fn pull(a: &Args) -> Option<podrunner_local::images::PodvmPull> {
    let binary = a.podvm.clone().or_else(|| a.guest.as_ref().map(|g| g.1.clone()))?;
    Some(podrunner_local::images::PodvmPull {
        binary,
        store: a.images.clone()?,
        platform: if super::arch() == "aarch64" { "linux/arm64".into() } else { "linux/amd64".into() },
    })
}

/// The fix for a job with containers on this host.
const FIX: &str = "run it with --podvm where `podrunner doctor --run` shows `ok containers`, or with --microvm and --engine (a host with /dev/kvm)";

/// The refusals of the run, as entries of its pins. `hooks` says whether
/// podvm gives container hooks here (its probe); it is read for `--podvm`
/// only.
pub(super) fn refusals(a: &Args, hooks: impl FnOnce() -> bool) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if !a.platforms.is_empty() {
        return out;
    }
    let containers = match (&a.podvm, &a.guest) {
        (None, None) => Some(format!(
            "the job needs containers (container:, services: or a docker:// step), and a runner directory runs none: {FIX}"
        )),
        (Some(_), None) if !hooks() => {
            Some("the job needs containers, and podvm gives no container hooks on this host (`podrunner doctor --run` names it): run it with --microvm and --engine (a host with /dev/kvm)".to_string())
        }
        (None, Some((true, _))) if !super::guest::has_engine(a) => {
            Some(format!(
                "the job needs containers: --microvm needs an image with an engine and podvm {} or later, or --engine DIR",
                podrunner_podvm::probe::LOWEST_IMAGE_LABELS
            ))
        }
        (None, Some((false, _))) if !super::guest::has_engine(a) => {
            Some("the job needs containers: --incus needs an image with an engine or --engine DIR".to_string())
        }
        _ => None,
    };
    if let Some(c) = containers {
        out.insert(REFUSE_CONTAINERS.to_string(), c);
    }
    if a.podvm.is_some() {
        out.insert(
            REFUSE_SERVICES.to_string(),
            "the job has services:, and a podvm unit of the local mode is on the host network, so it runs no services: run it with --microvm and --engine (a host with /dev/kvm)".to_string(),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-330: each executor that cannot run containers refuses them by one
    /// text with the fix.
    #[test]
    fn each_executor_names_what_it_cannot_run() {
        let dir = Args { runner_dir: Some("/r".into()), ..Args::default() };
        assert!(refusals(&dir, || true)[REFUSE_CONTAINERS].contains("doctor --run"));
        let podvm = Args { podvm: Some("/p".into()), ..Args::default() };
        let got = refusals(&podvm, || true);
        assert!(!got.contains_key(REFUSE_CONTAINERS) && got[REFUSE_SERVICES].contains("--microvm"));
        let no_hooks = refusals(&podvm, || false);
        assert!(
            no_hooks[REFUSE_CONTAINERS].contains("no container hooks")
                && !no_hooks[REFUSE_CONTAINERS].contains("--podvm")
        );
        let routed = Args { platforms: vec![("x".into(), "host".into())], ..dir };
        assert!(refusals(&routed, || false).is_empty(), "--platform: each executor refuses");
    }
}
