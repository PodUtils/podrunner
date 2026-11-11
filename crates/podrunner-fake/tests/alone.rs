//! The unit tests that see each process of the uid (`conformance::ALONE`):
//! the fake unit passes them, and each plant fails its test. This binary
//! has one test, so no other test of it runs in parallel.

use std::collections::BTreeMap;
use std::path::PathBuf;

use podrunner_fake::{FakeUnit, UnitPlant, payload};
use podrunner_schema::{
    Boundary, Capability, ContainerRoute, CredentialStore, EgressHold, Fact, LimitMode, Principal, SecretDelivery,
    TreeKill, UnitId, UnitIds,
};
use podrunner_unit::UnitDriver;
use podrunner_unit::conformance::{self as uc, Behaviour, Outcome};

struct Fixture {
    unit: FakeUnit,
    ids: UnitIds,
    dir: PathBuf,
}

impl Fixture {
    fn new(n: u32, plant: Option<UnitPlant>) -> Fixture {
        let dir = std::env::temp_dir().join(format!("podrunner-alone-{}-{n}-{}", std::process::id(), plant.is_some()));
        std::fs::create_dir_all(&dir).unwrap();
        let capability = Capability {
            schema: 1,
            kind: "fake".into(),
            arch: "x86_64".into(),
            boundary: Boundary::Redirect,
            boundary_proof: None,
            principal: Principal::Shared,
            container: ContainerRoute::Hooks,
            net: "shared-host".into(),
            secret_delivery: vec![SecretDelivery::Files],
            credential_store: CredentialStore::PrivateDir,
            mount_modes: vec!["rw".into()],
            rootfs_fresh: Fact::Yes,
            tree_kill: TreeKill::Unverified,
            limits: BTreeMap::from([("wall".into(), LimitMode::Enforced)]),
            egress_hold: EgressHold::None,
            kvm: Fact::No,
        };
        let mut unit = FakeUnit::new(&dir, capability, "linux/amd64");
        if let Some(p) = plant {
            unit = unit.with_plant(p);
        }
        Fixture { unit, ids: UnitIds::default(), dir }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl uc::Fixture for Fixture {
    fn driver(&self) -> &dyn UnitDriver {
        &self.unit
    }
    fn image(&self) -> String {
        format!("stand-in@sha256:{}", "1".repeat(64))
    }
    fn platform(&self) -> String {
        "linux/amd64".into()
    }
    fn foreign_platform(&self) -> String {
        "linux/s390x".into()
    }
    fn payload(&self, b: Behaviour) -> Vec<String> {
        payload(b)
    }
    fn unit(&self) -> UnitId {
        self.ids.next()
    }
    fn state_dir(&self) -> Option<PathBuf> {
        Some(self.dir.clone())
    }
}

#[test]
fn the_tests_that_see_each_process() {
    let plants = [(23, UnitPlant::ProbeSpawns), (24, UnitPlant::CredentialInContainers)];
    assert_eq!(plants.map(|p| p.0), uc::ALONE);
    for (n, plant) in plants {
        let r = uc::run(&Fixture::new(n, None), &[n]).remove(0);
        assert_eq!(r.outcome, Outcome::Pass, "{r}");
        let fx = Fixture::new(n, Some(plant));
        let r = uc::run(&fx, &[n]).remove(0);
        assert!(matches!(r.outcome, Outcome::Fail(_)), "{plant:?} passes test {n}: {r}");
    }
}
