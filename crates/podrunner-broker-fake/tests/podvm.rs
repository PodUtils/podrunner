//! The official runner in a podvm unit against the fake broker (T-026):
//! the podvm driver starts the runner image with the JIT files of the
//! broker, and the runner takes one canned job and completes it. Ignored
//! by default: it needs podvm, a store with the runner image, and openssl.
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_RUNNER_IMAGE=NAME@sha256:HEX \
//!   cargo test -p podrunner-broker-fake --test podvm -- --ignored
//!
//! The image is the one of `images/runner/`, or for a test on a host with
//! no docker the stand-in of `scripts/runner-stand-in.py`.

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use podrunner_broker_fake::{Broker, Job, files::runner_files};
use podrunner_podvm::{Podvm, PodvmUnit};
use podrunner_schema::{Credential, CredentialFile, Secret, UnitId};
use podrunner_unit::{StartSpec, UnitDriver};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[test]
#[ignore]
fn the_runner_in_a_podvm_unit_runs_a_canned_job() {
    let state = std::env::temp_dir().join(format!("podrunner-podvm-runner-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let podvm = Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) };
    let unit = PodvmUnit::new(podvm, &state, &PathBuf::from(env("PODRUNNER_PODVM_IMAGES"))).unwrap();
    let image = env("PODRUNNER_RUNNER_IMAGE");
    let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let prepared = unit.prepare(&image, platform, false).unwrap();

    let broker = Broker::start().unwrap();
    broker.queue(Job::scripts("in-podvm", &["echo podrunner-podvm-$((6*7))"]));
    let files = runner_files(&broker.base, "runner-u-1", &common::rsaparams())
        .into_iter()
        .map(|(name, text)| CredentialFile { name, content: Secret::new(text) })
        .collect();
    let id = UnitId::parse("u-1").unwrap();
    let spec = StartSpec {
        unit: id.clone(),
        image: prepared,
        argv: vec!["/home/runner/run.sh".into()],
        env: Default::default(),
        credential: Credential { files },
        sockets: None,
    };
    unit.start(&spec).unwrap();
    let exit = unit.wait(&id, Duration::from_secs(300)).unwrap();
    let t = Instant::now();
    while broker.completions().is_empty() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let done = broker.completions();
    let logs = broker.logs();
    // The output of the runner in the unit, for a failure.
    let unit_log = std::process::Command::new(env("PODRUNNER_PODVM"))
        .args(["logs", "u-1"])
        .env("PODVM_STORE", state.join("units").join("u-1").join("store"))
        .output()
        .map(|o| String::from_utf8_lossy(&[o.stdout, o.stderr].concat()).into_owned())
        .unwrap_or_default();
    println!("{unit_log}");
    let residue = unit.destroy(&id).unwrap();
    let _ = std::fs::remove_dir_all(&state);
    println!("exit {exit:?}, residue {residue:?}\n{:?}", broker.seen());
    assert_eq!(done.len(), 1, "{:?}\n{logs}", broker.seen());
    assert_eq!(done[0].conclusion, "succeeded", "{logs}");
    assert!(logs.contains("podrunner-podvm-42"), "{logs}");
    assert!(residue.is_clean(), "{residue:?}");
}
