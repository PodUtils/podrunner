//! T-648: the mode of a job on a kind that mounts the cache, holds `:ro`
//! and lists no `rw` (the microvm and incus kinds): a writer reads.

use podrunner_schema::{Boundary, EgressHold, JobOffer, Principal, Tier};
use serde_json::{Value, json};

use super::store::Mode;
use super::{CacheConfig, Mounts, decide};

/// The microvm and incus kinds: the cache at `ro` only.
const RO_ONLY: Mounts = Mounts { host: true, ro: true, rw: false };

fn offer(with: Value) -> JobOffer {
    let step: Value = serde_json::from_str(&crate::tests::offer(0, "j-1", with)).unwrap();
    serde_json::from_value(step["offer"].clone()).unwrap()
}

fn cfg() -> CacheConfig {
    #[derive(serde::Deserialize)]
    struct File {
        cache: CacheConfig,
    }
    toml::from_str::<File>("[cache]").unwrap().cache
}

fn mode(tier: Tier, with: Value, mounts: Mounts) -> Result<Mode, &'static str> {
    decide(&cfg(), tier, &offer(with), mounts).map(|d| d.0).map_err(|n| n.code)
}

#[test]
fn a_kind_with_no_rw_gives_a_writer_ro_and_a_reader_ro_not_none() {
    let push = json!({});
    let asked_rw = json!({ "cache": { "key": "k", "mode": "rw" } });
    let pr = json!({ "trust_inputs": { "event": "pull_request", "ref": "refs/pull/7/merge" } });
    for tier in [Tier::Trusted, Tier::Local] {
        assert_eq!(mode(tier, push.clone(), RO_ONLY), Ok(Mode::Ro), "a push to main reads: {tier:?}");
        assert_eq!(mode(tier, asked_rw.clone(), RO_ONLY), Ok(Mode::Ro), "a source that says rw: {tier:?}");
        assert_eq!(mode(tier, pr.clone(), RO_ONLY), Ok(Mode::Ro), "a pull request: {tier:?}");
        // The same kind with `rw` keeps the writer.
        assert_eq!(mode(tier, push.clone(), Mounts { rw: true, ..RO_ONLY }), Ok(Mode::Rw));
    }
    // The other rules hold before the mode.
    assert_eq!(mode(Tier::Untrusted, push.clone(), RO_ONLY), Err("untrusted"));
    assert_eq!(mode(Tier::Trusted, json!({ "cache": { "key": "k", "mode": "none" } }), RO_ONLY), Err("offer-off"));
    // A kind that holds neither mode gives no cache: a write would reach it.
    let neither = Mounts { host: true, ro: false, rw: false };
    assert_eq!(mode(Tier::Trusted, push, neither), Err("no-ro-mount"));
}

#[test]
fn the_mounts_of_a_kind_come_from_its_driver_and_its_record() {
    let dir = crate::tests::TempDir::new();
    let mut cap = crate::tests::capability(Boundary::Guest, Principal::Distinct, EgressHold::Enforced);
    let unit = podrunner_fake::FakeUnit::new(&dir.0.join("fake"), cap.clone(), "linux/amd64");
    // The record of the microvm and incus kinds.
    cap.mount_modes = vec!["ro".into()];
    assert_eq!(Mounts::of(&unit, &cap), RO_ONLY);
    cap.mount_modes = vec!["ro".into(), "rw".into()];
    assert_eq!(Mounts::of(&unit, &cap), Mounts { host: true, ro: true, rw: true });
    cap.mount_modes = vec!["rw".into()];
    assert_eq!(Mounts::of(&unit, &cap), Mounts { host: true, ro: false, rw: true });
}
