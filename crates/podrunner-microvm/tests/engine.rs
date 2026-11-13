//! A container engine in a guest of the microvm kind (T-103): the guest
//! script starts `dockerd` from the read-only engine directory, and the
//! payload, as the user `runner`, runs a container through `DOCKER_HOST`.
//! Ignored by default: it needs podvm `v0.2.0-beta.14` or later, QEMU, a
//! store with the runner image, and a directory with the static Docker
//! binaries and a saved image `measure:latest` (`image.tar`).
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_PODVM_IMAGE=runner@sha256:HEX PODRUNNER_ENGINE_DIR=DIR \
//!   cargo test -p podrunner-microvm --test engine -- --ignored --nocapture

use std::path::PathBuf;
use std::time::Duration;

use podrunner_microvm::{ENGINE_MOUNT, MicrovmUnit};
use podrunner_podvm::Podvm;
use podrunner_schema::{Credential, UnitId};
use podrunner_unit::{StartSpec, UnitDriver};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[test]
#[ignore]
fn the_runner_user_runs_a_container_on_the_engine_of_its_guest() {
    let state = std::env::temp_dir().join(format!("podrunner-microvm-engine-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let unit = MicrovmUnit::new(
        Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) },
        &state,
        &PathBuf::from(env("PODRUNNER_PODVM_IMAGES")),
    )
    .with_engine(&PathBuf::from(env("PODRUNNER_ENGINE_DIR")))
    .with_kept_logs(&state.join("logs"));
    let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let image = unit.prepare(&env("PODRUNNER_PODVM_IMAGE"), platform, false).unwrap();
    let script = format!(
        "id -un; echo \"host $DOCKER_HOST\"; docker info --format 'driver {{{{.Driver}}}}'; \
         docker load -i {ENGINE_MOUNT}/image.tar >/dev/null && echo loaded; \
         docker network create svc >/dev/null && echo network; \
         docker run --rm --network svc measure:latest sh -c 'echo engine-$((6*7))'"
    );
    let id = UnitId::parse("u-1").unwrap();
    let spec = StartSpec {
        unit: id.clone(),
        image,
        argv: vec!["sh".into(), "-c".into(), script],
        env: Default::default(),
        credential: Credential { files: vec![] },
        sockets: None,
    };
    unit.start(&spec).unwrap();
    let exit = unit.wait(&id, Duration::from_secs(600)).unwrap();
    let log = String::from_utf8_lossy(&unit.unit_logs(&id).unwrap_or_default()).to_string();
    let residue = unit.destroy(&id).unwrap();
    let _ = std::fs::remove_dir_all(&state);
    println!("exit {exit:?}, residue {residue:?}\n{log}");
    assert!(log.contains("runner"), "the payload runs as runner: {log}");
    assert!(log.contains("host unix:///run/podrunner-docker.sock"), "{log}");
    assert!(log.contains("driver overlay2"), "{log}");
    assert!(log.contains("engine-42"), "{log}");
    assert!(residue.is_clean(), "{residue:?}");
}
