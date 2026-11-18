//! A limit of processes in a guest of the microvm kind (T-488, githome's
//! G-12): podvm `0.2.0-beta.32` puts the command of the guest in a cgroup
//! with `pids.max` (`--pids-limit`). A payload that forks past it fails,
//! and the same payload with no limit passes (the control). Ignored by
//! default: it needs podvm, QEMU and a store with the runner image.
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_PODVM_IMAGE=NAME@sha256:HEX \
//!   cargo test -p podrunner-microvm --test pids -- --ignored --nocapture

use std::path::PathBuf;
use std::time::Duration;

use podrunner_microvm::MicrovmUnit;
use podrunner_podvm::Podvm;
use podrunner_schema::UnitId;
use podrunner_unit::{Exit, StartSpec, UnitDriver};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// 300 processes at one time: exit 12 when each fork passed.
const FORKS: &str = "i=0; while [ $i -lt 300 ]; do sleep 30 & i=$((i+1)); done; exit 12";

fn run(n: u32, pids: Option<u32>) -> Option<Exit> {
    let state = std::env::temp_dir().join(format!("podrunner-microvm-pids-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let unit = MicrovmUnit::new(
        Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) },
        &state,
        &PathBuf::from(env("PODRUNNER_PODVM_IMAGES")),
    );
    // The default size of the kind: the initrd of the runner image needs
    // about 3.3 GiB of memory (run 44349725647 with 1G: exit 125).
    let unit = match pids {
        Some(p) => unit.with_pids(p),
        None => unit,
    };
    let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let image = unit.prepare(&env("PODRUNNER_PODVM_IMAGE"), platform, false).unwrap();
    let id = UnitId::parse(&format!("u-{n}")).unwrap();
    let spec = StartSpec {
        unit: id.clone(),
        image,
        argv: ["sh", "-c", FORKS].map(String::from).to_vec(),
        env: Default::default(),
        credential: Default::default(),
        sockets: None,
        mounts: Vec::new(),
        limits: Default::default(),
    };
    unit.start(&spec).unwrap();
    let exit = unit.wait(&id, Duration::from_secs(300)).unwrap();
    let log = String::from_utf8_lossy(&unit.unit_logs(&id).unwrap_or_default()).to_string();
    let residue = unit.destroy(&id).unwrap();
    let _ = std::fs::remove_dir_all(&state);
    println!("guest {n} pids {pids:?}: {exit:?}\n{log}");
    assert!(residue.is_clean(), "{residue:?}");
    exit
}

#[test]
#[ignore]
fn a_payload_past_the_pids_limit_of_a_guest_gets_an_error() {
    assert_eq!(run(1, None), Some(Exit::Code(12)), "the control: 300 forks pass with no limit");
    let exit = run(2, Some(64));
    assert!(matches!(exit, Some(Exit::Code(c)) if c != 0 && c != 12), "300 forks must fail under pids 64: {exit:?}");
}
