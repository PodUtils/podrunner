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

/// The lowest podvm of the `microvm` and `incus` kinds: the channel of a
/// guest with no NIC, and `run -d -i` of an incus instance (T-110).
pub const LOWEST_GUEST: &str = "0.2.0-beta.10";

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
        // Containers need volumes, and podvm gives volumes on the supervise
        // rung only (U19, T-034).
        container: if matches!(rung, "supervise" | "userland") { ContainerRoute::Hooks } else { ContainerRoute::None },
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

#[cfg(test)]
mod tests {
    use super::*;

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
