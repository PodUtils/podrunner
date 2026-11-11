//! The unit conformance suite on a real podvm, with a stand-in payload
//! (busybox). Ignored by default: CI has no podvm. Run:
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_PODVM_IMAGE=busybox@sha256:HEX \
//!   cargo test -p podrunner-podvm --test conformance -- --ignored
//!
//! STORE is a podvm store that holds the image (`podvm pull`).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use podrunner_podvm::{Podvm, PodvmUnit};
use podrunner_schema::{UnitId, UnitIds};
use podrunner_unit::UnitDriver;
use podrunner_unit::conformance::{self, Behaviour, Fixture, Outcome};

struct Real {
    unit: PodvmUnit,
    image: String,
    ids: UnitIds,
}

fn sh(s: &str) -> Vec<String> {
    vec!["sh".into(), "-c".into(), s.into()]
}

impl Fixture for Real {
    fn driver(&self) -> &dyn UnitDriver {
        &self.unit
    }
    fn image(&self) -> String {
        self.image.clone()
    }
    fn platform(&self) -> String {
        "linux/amd64".into()
    }
    fn foreign_platform(&self) -> String {
        "linux/s390x".into()
    }
    fn payload(&self, b: Behaviour) -> Vec<String> {
        match b {
            Behaviour::Exit(n) => sh(&format!("exit {n}")),
            Behaviour::Sleep => vec!["sleep".into(), "300".into()],
            Behaviour::Grandchild => sh("setsid sh -c 'sleep 300 & wait' & wait"),
            Behaviour::Marker => sh(&format!("echo x > {}; exec sleep 300", conformance::MARKER)),
        }
    }
    fn unit(&self) -> UnitId {
        self.ids.next()
    }
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[test]
#[ignore]
fn the_podvm_unit_passes_its_tests() {
    static N: AtomicU64 = AtomicU64::new(0);
    let state = std::env::temp_dir().join(format!(
        "podrunner-podvm-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let podvm = Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) };
    let mut unit = PodvmUnit::new(podvm, &state, &PathBuf::from(env("PODRUNNER_PODVM_IMAGES"))).unwrap();
    let image = env("PODRUNNER_PODVM_IMAGE");
    // The self-test of the tree kill; test 17 then holds the claim to test 3.
    assert!(unit.verify_tree_kill(&image, UnitId::parse("u-999").unwrap()).unwrap(), "the tree kill is not proven");
    assert_eq!(unit.probe().unwrap().tree_kill, podrunner_schema::TreeKill::Verified);
    let fx = Real { unit, image, ids: UnitIds::default() };
    let results = conformance::run(&fx, &[]);
    for r in &results {
        println!("{r}");
    }
    // The container tests (13 or 14, and 20) skip by the container route of
    // the rung; the tests 18 and 21 skip: the fixture names no state directory.
    let failed: Vec<_> = results.iter().filter(|r| matches!(r.outcome, Outcome::Fail(_))).map(|r| r.number).collect();
    let _ = std::fs::remove_dir_all(&state);
    assert!(failed.is_empty(), "failed: {failed:?}");
}
