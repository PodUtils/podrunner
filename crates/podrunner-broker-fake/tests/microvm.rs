//! The official runner in a guest of the microvm kind against the fake
//! broker (T-037, step 3): the guest reaches the broker on the loopback of
//! the host at its gateway, takes one canned job and completes it. Ignored
//! by default: it needs podvm, QEMU and a store with the runner image.
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_RUNNER_IMAGE=NAME@sha256:HEX \
//!   cargo test -p podrunner-broker-fake --test microvm -- --ignored --nocapture

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use podrunner_broker_fake::{Broker, Job, files::runner_files};
use podrunner_microvm::MicrovmUnit;
use podrunner_podvm::Podvm;
use podrunner_schema::{Credential, CredentialFile, Secret, UnitId};
use podrunner_unit::{StartSpec, UnitDriver};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[test]
#[ignore]
fn the_runner_in_a_guest_runs_a_canned_job() {
    let state = std::env::temp_dir().join(format!("podrunner-microvm-runner-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let podvm = Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) };
    let unit = MicrovmUnit::new(podvm, &state, &PathBuf::from(env("PODRUNNER_PODVM_IMAGES")));
    let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let prepared = unit.prepare(&env("PODRUNNER_RUNNER_IMAGE"), platform, false).unwrap();

    let broker = Broker::start_seen_at("10.0.2.2").unwrap();
    broker.queue(Job::scripts("in-guest", &["echo podrunner-guest-$((6*7)); uname -r"]));
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
    let exit = unit.wait(&id, Duration::from_secs(600)).unwrap();
    let t = Instant::now();
    while broker.completions().is_empty() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let (done, logs) = (broker.completions(), broker.logs());
    println!("{}", String::from_utf8_lossy(&unit.unit_logs(&id).unwrap_or_default()));
    let residue = unit.destroy(&id).unwrap();
    let _ = std::fs::remove_dir_all(&state);
    println!("exit {exit:?}, residue {residue:?}\n{:?}", broker.seen());
    assert_eq!(done.len(), 1, "{:?}\n{logs}", broker.seen());
    assert_eq!(done[0].conclusion, "succeeded", "{logs}");
    assert!(logs.contains("podrunner-guest-42"), "{logs}");
    assert!(residue.is_clean(), "{residue:?}");
}
