//! The capability record of a podvm unit, from `podvm probe --json`, and
//! the version range of podvm that this driver knows.
//!
//! Each podvm rung is `redirect`: path rewriting, no isolation against
//! hostile code. A field that the driver cannot map from the probe is
//! `unknown`. The tree kill stays `unverified` until unit test 3 passes on
//! the host; with it, a `wall` limit is only `reported`.

use podrunner_schema::{
    Boundary, Capability, ContainerRoute, CredentialStore, EgressHold, Fact, Principal, SCHEMA, SecretDelivery,
    TreeKill,
};
use podrunner_unit::{UnitCode, UnitError};
use serde_json::Value;

/// The lowest version of podvm that this driver knows (inclusive), and the
/// first core version that it does not (exclusive, with each of its
/// pre-releases). A beta of podvm can rename a
/// flag: a version outside the range fails `probe` by name, not a job at
/// run time. beta.4 gives a tree for each container (T-064), resolves an
/// absolute link in the rootfs (T-061) and applies `:ro` on `exec`
/// (T-062); beta.5 keeps the labels of `run` and writes its report (T-078).
pub const LOWEST: &str = "0.2.0-beta.5";
pub const BEYOND: &str = "0.3.0";

/// A version `MAJOR.MINOR.PATCH[-PRE.N]`, ordered as semver orders it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    core: (u64, u64, u64),
    /// No pre-release sorts after each pre-release of the same core.
    pre: (bool, String, u64),
}

impl Version {
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim().trim_start_matches("podvm ").trim_start_matches('v');
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (s, None),
        };
        let mut n = core.split('.').map(|x| x.parse::<u64>().ok());
        let core = (n.next()??, n.next()??, n.next()??);
        if n.next().is_some() {
            return None;
        }
        let pre = match pre {
            None => (true, String::new(), 0),
            Some(p) => {
                let (word, num) = p.rsplit_once('.').unwrap_or((p, "0"));
                (false, word.to_string(), num.parse().ok()?)
            }
        };
        Some(Version { core, pre })
    }
}

/// The lowest podvm of the `microvm` kind: the channel of a guest with no
/// NIC (T-110).
pub const LOWEST_GUEST: &str = "0.2.0-beta.10";
/// The lowest podvm of the `microvm` kind with an engine: `--podvm-engine`
/// (T-103, T-112).
pub const LOWEST_ENGINE: &str = "0.2.0-beta.14";

/// The first podvm that holds a limit of processes in a machine guest
/// (`--pids-limit` on the machine tier, PodUtils/podvm@T-1967, T-488).
pub const LOWEST_GUEST_PIDS: &str = "0.2.0-beta.32";

/// The first podvm that runs an engine in an incus instance
/// (`--podvm-engine` on the incus tier, PodUtils/podvm@T-1966, T-485).
pub const LOWEST_ENGINE_INCUS: &str = "0.2.0-beta.32";
/// The lowest podvm of the `incus` kind: `run -d -i`, and no NIC and one
/// channel on the incus tier with no proxy device (T-109).
pub const LOWEST_INCUS: &str = "0.2.0-beta.13";
/// The first podvm whose incus tier boots the init of the image as PID 1
/// (`--podvm-init=image`, PodUtils/podvm@T-1993, T-543).
pub const LOWEST_INCUS_INIT: &str = "0.2.0-beta.36";
/// The first podvm that gives an Incus warm claim its stdin through `exec -i` (T-487).
pub const LOWEST_INCUS_WARM: &str = "0.2.0-beta.39";
/// The first podvm whose machine tier runs the command beside the init of
/// an image whose Entrypoint is its init (`--podvm-init=image`,
/// PodUtils/podvm@T-2011, T-2048, T-549).
pub const LOWEST_MACHINE_INIT: &str = "0.2.0-beta.37";
/// The first podvm whose `exec` into a `run -d` container of the namespace
/// rung joins its namespaces, where its volumes are (PodUtils/podvm@T-2012,
/// COORD-113, T-532): a step of a container runs on that rung.
pub const LOWEST_NAMESPACE_EXEC: &str = "0.2.0-beta.37";
/// The first podvm whose `logs` streams and takes `--limit-bytes N`, also
/// on the incus tier (PodUtils/podvm@T-2046, T-570).
pub const LOWEST_LOG_LIMIT: &str = "0.2.0-beta.37";
/// The lowest podvm whose incus tier takes `--memory`, `--cpus` and
/// `--pids-limit` (PodUtils/podvm@T-1897, T-162).
pub const LOWEST_INCUS_LIMITS: &str = "0.2.0-beta.20";

/// True when `v` comes before the core version of `beyond`: each
/// pre-release of `beyond` is not before it.
pub fn before(v: &Version, beyond: &Version) -> bool {
    v.core < beyond.core
}

/// True when `v` is in the range of this driver.
pub fn in_range(v: &Version) -> bool {
    let low = Version::parse(LOWEST).expect("LOWEST");
    let beyond = Version::parse(BEYOND).expect("BEYOND");
    *v >= low && before(v, &beyond)
}

/// The capability record of a podvm unit on this host, from the output of
/// `podvm probe --json`.
pub fn capability(probe: &Value, arch: &str) -> Result<Capability, UnitError> {
    let unsupported = |why: String| UnitError::new(UnitCode::Unsupported, why);
    let version = probe["podvm"].as_str().ok_or_else(|| unsupported("the probe names no podvm version".into()))?;
    let v = Version::parse(version).ok_or_else(|| unsupported(format!("podvm {version:?} is not a version")))?;
    if !in_range(&v) {
        return Err(unsupported(format!("podvm {version} is outside {LOWEST} to {BEYOND} (exclusive)")));
    }
    let rung = probe["rung"].as_str().ok_or_else(|| unsupported("the probe names no rung".into()))?;
    if rung.is_empty() {
        return Err(unsupported("the probe names an empty rung".into()));
    }
    Ok(Capability {
        schema: SCHEMA,
        kind: "podvm".into(),
        arch: arch.into(),
        // Every rung of podvm rewrites paths; none isolates.
        boundary: Boundary::Redirect,
        boundary_proof: Some(format!("podvm-probe:{rung}")),
        // The units run as the uid of podrunner.
        principal: Principal::Shared,
        // Containers need volumes in each step: podvm gives them on the
        // supervise and userland rungs (U19, T-034). On the namespace rung
        // `run -d -v` binds, and from `LOWEST_NAMESPACE_EXEC` `exec` joins
        // the mount namespace of the container (T-532); before, `exec`
        // refused each volume, and the units got the socket volume only.
        container: container_route(rung, &v),
        net: "shared-host".into(),
        secret_delivery: vec![SecretDelivery::Files],
        // No tmpfs on a host with no mounts: a directory of mode 0700.
        credential_store: CredentialStore::PrivateDir,
        mount_modes: vec!["rw".into()],
        // Each unit has its own store, so its own rootfs.
        rootfs_fresh: Fact::Yes,
        tree_kill: TreeKill::Unverified,
        limits: Default::default(),
        // The hold of `podnet pod` is not wired into this driver.
        egress_hold: EgressHold::Unknown,
        kvm: Fact::Unknown,
    })
}

/// The route of the containers of a podvm unit on `rung` with podvm `v`.
fn container_route(rung: &str, v: &Version) -> ContainerRoute {
    let joins = Version::parse(LOWEST_NAMESPACE_EXEC).is_some_and(|low| *v >= low);
    match rung {
        "supervise" | "userland" => ContainerRoute::Hooks,
        "namespace" if joins => ContainerRoute::Hooks,
        _ => ContainerRoute::None,
    }
}

/// The rung of the probe in a capability (`boundary_proof`), with no note
/// of the hold or of the confine self-test.
pub fn rung(capability: &Capability) -> Option<&str> {
    let proof = capability.boundary_proof.as_deref()?.strip_prefix("podvm-probe:")?;
    Some(proof.split(';').next().unwrap_or("").trim())
}

/// The rungs where `run -d -v` binds a volume into a unit: the socket
/// volume of a unit (T-050, T-564; namespace since podvm 0.2.0-beta.25,
/// COORD-189). The rung is the one of the probe (`boundary_proof`).
pub fn has_volumes(capability: &Capability) -> bool {
    // The hold and the confine self-test add "; ..." after the rung.
    matches!(rung(capability), Some("supervise" | "userland" | "namespace"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rung_of(c: &Capability) -> Option<&str> {
        rung(c)
    }

    fn fixture(name: &str) -> Value {
        let path = format!("{}/tests/probe/{name}.json", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn versions_order_as_semver() {
        let v = |s| Version::parse(s).unwrap();
        assert!(v("0.2.0-beta.2") < v("0.2.0-beta.10"));
        assert!(v("0.2.0-beta.10") < v("0.2.0"));
        assert!(v("0.2.0") < v("0.3.0-beta.1"));
        assert_eq!(v("podvm 0.2.0-beta.2"), v("v0.2.0-beta.2"));
        assert!(Version::parse("0.2").is_none() && Version::parse("x.y.z").is_none());
        assert!(in_range(&v("0.2.0-beta.5")) && in_range(&v("0.2.9")));
        assert!(!in_range(&v("0.2.0-beta.4")) && !in_range(&v("0.3.0-beta.1")) && !in_range(&v("0.1.0-beta.12")));
    }

    /// T-564: `run -d -v` binds on the namespace rung since podvm
    /// `0.2.0-beta.25` (COORD-189), so its units get the socket volume.
    /// T-532: a step of a container needs `exec` with a volume, which the
    /// namespace rung refused before podvm `0.2.0-beta.37`; from it `exec`
    /// joins the container, so the container hooks are on that rung too.
    #[test]
    fn the_rungs_with_volumes_and_the_rungs_with_containers() {
        for (rung, version, hooks) in [
            ("namespace", "0.2.0-beta.36", false),
            ("namespace", "0.2.0-beta.37", true),
            ("userland", "0.2.0-beta.5", true),
            ("supervise", "0.2.0-beta.5", true),
        ] {
            let mut probe = fixture(rung);
            probe["podvm"] = Value::String(version.into());
            let c = capability(&probe, "x86_64").unwrap();
            assert!(has_volumes(&c), "{rung}");
            assert_eq!(rung_of(&c), Some(rung));
            let want = if hooks { ContainerRoute::Hooks } else { ContainerRoute::None };
            assert_eq!(c.container, want, "{rung} {version}");
        }
        let mut chroot = fixture("namespace");
        chroot["rung"] = Value::String("chroot".into());
        let c = capability(&chroot, "x86_64").unwrap();
        assert!(!has_volumes(&c) && c.container == ContainerRoute::None);
        // The hold and a verified confine add to the proof after the rung.
        let mut held = capability(&fixture("supervise"), "x86_64").unwrap();
        held.boundary_proof = Some("podvm-probe:supervise; hold: podnet pod seccomp advisory".into());
        assert!(has_volumes(&held), "{:?}", held.boundary_proof);
        assert_eq!(rung_of(&held), Some("supervise"));
    }

    #[test]
    fn each_rung_is_redirect_and_unknown_stays_unknown() {
        for rung in ["namespace", "userland", "supervise"] {
            let c = capability(&fixture(rung), "x86_64").unwrap();
            assert_eq!(c.boundary, Boundary::Redirect, "{rung}");
            assert_eq!(c.boundary_proof.as_deref(), Some(format!("podvm-probe:{rung}").as_str()));
            assert_eq!(c.tree_kill, TreeKill::Unverified);
            assert_eq!(c.egress_hold, EgressHold::Unknown);
            assert_eq!(c.mount_modes, ["rw"]);
            assert!(c.check().is_ok());
        }
    }

    #[test]
    fn a_podvm_outside_the_range_fails_the_probe_by_name() {
        let e = capability(&fixture("old"), "x86_64").unwrap_err();
        assert_eq!(e.code, UnitCode::Unsupported);
        assert!(e.detail.contains("0.1.0-beta.12 is outside"), "{e}");
        let mut no_rung = fixture("namespace");
        no_rung["rung"] = Value::Null;
        assert!(capability(&no_rung, "x86_64").is_err());
    }
}
