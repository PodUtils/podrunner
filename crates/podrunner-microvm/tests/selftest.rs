//! The guest floor self-test of T-036 on a real guest of the microvm kind
//! (GitHub #36): no NIC but `lo`, no file or process of the host or of
//! another unit, a kernel of its own, and no reach to the loopback of the
//! host. Ignored by default: it needs podvm and QEMU.
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_PODVM_IMAGE=runner@sha256:HEX \
//!   cargo test -p podrunner-microvm --test selftest -- --ignored --nocapture

use std::path::PathBuf;

use podrunner_microvm::MicrovmUnit;
use podrunner_podvm::Podvm;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[test]
#[ignore]
fn a_guest_of_the_microvm_kind_passes_the_guest_self_test() {
    let state = std::env::temp_dir().join(format!("podrunner-microvm-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(&state).unwrap();
    let podvm = Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) };
    let unit = MicrovmUnit::new(podvm, &state, &PathBuf::from(env("PODRUNNER_PODVM_IMAGES")));
    let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let verdict = podrunner_slots::guest::self_test(
        &unit,
        &env("PODRUNNER_PODVM_IMAGE"),
        platform,
        podrunner_slots::guest::test_ids(),
        &state,
    );
    let _ = std::fs::remove_dir_all(&state);
    assert_eq!(verdict, Ok(()));
}
