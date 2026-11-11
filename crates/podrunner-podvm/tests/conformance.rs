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
    let unit = PodvmUnit::new(podvm, &state, &PathBuf::from(env("PODRUNNER_PODVM_IMAGES"))).unwrap();
    let fx = Real { unit, image: env("PODRUNNER_PODVM_IMAGE"), ids: UnitIds::default() };
    let results = conformance::run(&fx, &[]);
    for r in &results {
        println!("{r}");
    }
    // Test 14 skips: the podvm unit has no container route in v1.
    let failed: Vec<_> = results.iter().filter(|r| matches!(r.outcome, Outcome::Fail(_))).map(|r| r.number).collect();
    let _ = std::fs::remove_dir_all(&state);
    assert!(failed.is_empty(), "failed: {failed:?}");
}
