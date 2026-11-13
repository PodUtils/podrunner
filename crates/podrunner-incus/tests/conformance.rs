//! The unit conformance tests of T-105 on real system containers of the
//! incus tier of podvm. The processes of an instance belong to incusd, not
//! to podrunner, so the tests that count processes of the host (3, 7, 10)
//! do not apply; 27 proves the claim `confined`. Ignored by default: it
//! needs podvm `v0.2.0-beta.10` or later and an incusd.
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_PODVM_IMAGE=runner@sha256:HEX \
//!   cargo test -p podrunner-incus --test conformance -- --ignored --nocapture
//!
//! STORE is a podvm store that holds the image (`podvm load`).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use podrunner_incus::IncusUnit;
use podrunner_podvm::Podvm;
use podrunner_schema::{UnitId, UnitIds};
use podrunner_unit::UnitDriver;
use podrunner_unit::conformance::{self, Behaviour, Fixture, Outcome};

/// The tests of the kind.
const TESTS: [u32; 14] = [1, 2, 4, 5, 6, 8, 9, 11, 12, 13, 15, 16, 22, 27];

struct Real {
    unit: IncusUnit,
    image: String,
    ids: UnitIds,
    state: PathBuf,
    /// Each unit that a test made: its log is printed when a test fails.
    made: std::sync::Mutex<Vec<UnitId>>,
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
        if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" }.into()
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
            // An orphan stays in the instance: `rm -f` ends it.
            Behaviour::Escape | Behaviour::KillParent => sh("(setsid sleep 300 &); exec sleep 300"),
            Behaviour::ReadOthers => sh("exec sleep 300"),
        }
    }
    fn unit(&self) -> UnitId {
        let id = self.ids.next();
        self.made.lock().unwrap().push(id.clone());
        id
    }
    fn state_dir(&self) -> Option<PathBuf> {
        Some(self.state.join("units"))
    }
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[test]
#[ignore]
fn the_incus_unit_passes_its_tests() {
    let state = std::env::temp_dir().join(format!("podrunner-incus-{}", std::process::id()));
    let podvm = Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) };
    let unit = IncusUnit::new(podvm, &state, &PathBuf::from(env("PODRUNNER_PODVM_IMAGES")));
    let real = Arc::new(Real {
        unit,
        image: env("PODRUNNER_PODVM_IMAGE"),
        ids: UnitIds::default(),
        state: state.clone(),
        made: Default::default(),
    });
    let fx: Arc<dyn Fixture + Send + Sync> = real.clone();
    let mut failed = Vec::new();
    for n in TESTS {
        let name = conformance::tests().into_iter().find(|(m, _)| *m == n).map(|(_, s)| s).unwrap_or("?");
        println!("incus {n:2} {name}: running");
        real.made.lock().unwrap().clear();
        let r = conformance::run_bounded(fx.clone(), n, Duration::from_secs(300)).expect("a test");
        println!("{r}");
        if matches!(r.outcome, Outcome::Fail(_)) {
            failed.push(n);
            // The log of each guest of the test, kept at its destroy.
            for id in real.made.lock().unwrap().iter() {
                if let Ok(log) = std::fs::read(state.join("logs").join(format!("{id}.log"))) {
                    println!("--- the log of {id}:\n{}", String::from_utf8_lossy(&log));
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&state);
    assert!(failed.is_empty(), "failed: {failed:?}");
}
