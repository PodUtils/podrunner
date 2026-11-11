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
    state: PathBuf,
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
            Behaviour::Escape => sh(&podrunner_fake_escape("")),
            Behaviour::KillParent => sh(&podrunner_fake_escape("kill -9 $PPID; ")),
            Behaviour::ReadOthers => sh(&format!(
                "cat ../*/{m} > {s} 2>/dev/null; echo own > {o}.part; cat {o}.part > {o}; exec sleep 300",
                m = conformance::MARKER,
                s = conformance::SEEN,
                o = conformance::OWN
            )),
        }
    }
    fn unit(&self) -> UnitId {
        self.ids.next()
    }
    fn state_dir(&self) -> Option<PathBuf> {
        Some(self.state.join("units"))
    }
}

/// The script of the payloads with an orphan (as `podrunner_fake::escape`).
fn podrunner_fake_escape(then: &str) -> String {
    // The pid file comes last: when it is there, `then` is done. Shell
    // builtins only: a small image has no `mv`.
    let f = conformance::ESCAPED;
    format!("(setsid env -i sleep 300 & echo $! > {f}.part); {then}read p < {f}.part; echo $p > {f}; exec sleep 300")
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
    let fx = Real { unit, image, ids: UnitIds::default(), state: state.clone() };
    // One test at a time, each printed when it ends: a hang names its test.
    let mut results = Vec::new();
    for (n, name) in conformance::tests() {
        println!("unit {n:2} {name}: running");
        let r = conformance::run(&fx, &[n]).remove(0);
        println!("{r}");
        results.push(r);
    }
    // The container tests (13 or 14, 20 and 24) skip by the container route
    // of the rung.
    let failed: Vec<_> = results.iter().filter(|r| matches!(r.outcome, Outcome::Fail(_))).map(|r| r.number).collect();
    let _ = std::fs::remove_dir_all(&state);
    assert!(failed.is_empty(), "failed: {failed:?}");
}
