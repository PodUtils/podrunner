//! An absolute link in the rootfs of a podvm unit resolves in that rootfs,
//! not on the host (T-061, podvm `v0.2.0-beta.4`). The host puts a file
//! and an absolute link to it in the root of the unit; the payload reads
//! the link. Ignored by default. Run:
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_PODVM_IMAGE=NAME@sha256:HEX \
//!   cargo test -p podrunner-podvm --test links -- --ignored
//!
//! The image needs sh, cat and sleep.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use podrunner_podvm::{Podvm, PodvmUnit};
use podrunner_schema::UnitId;
use podrunner_unit::{StartSpec, UnitDriver};

const BODY: &str = "podrunner-in-the-rootfs";

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[test]
#[ignore]
fn an_absolute_link_resolves_in_the_rootfs_of_the_unit() {
    let state = std::env::temp_dir().join(format!("podrunner-links-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let podvm = Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) };
    let unit = PodvmUnit::new(podvm, &state, Path::new(&env("PODRUNNER_PODVM_IMAGES"))).unwrap();
    let image = unit.prepare(&env("PODRUNNER_PODVM_IMAGE"), "linux/amd64", false).unwrap();
    let id = UnitId::parse("u-1").unwrap();
    // The payload waits for the link, then reads it.
    let script = "while [ ! -e ready ]; do sleep 0.1; done; cat link > read.txt 2>&1; exec sleep 120";
    let spec = StartSpec {
        unit: id.clone(),
        image,
        argv: ["sh", "-c", script].map(String::from).to_vec(),
        env: Default::default(),
        credential: Default::default(),
        sockets: None,
        mounts: Vec::new(),
    };
    unit.start(&spec).unwrap();
    let work = unit.observe(&id).unwrap().unit_root.unwrap();
    // The working directory is one level under the root of the unit.
    let rootfs = work.parent().unwrap();
    let name = format!("podrunner-link-target-{}", std::process::id());
    std::fs::write(rootfs.join(&name), BODY).unwrap();
    std::os::unix::fs::symlink(format!("/{name}"), work.join("link")).unwrap();
    std::fs::write(work.join("ready"), "").unwrap();
    let end = Instant::now() + Duration::from_secs(30);
    let mut read = String::new();
    while Instant::now() < end && !read.contains(BODY) {
        read = std::fs::read_to_string(work.join("read.txt")).unwrap_or_default();
        std::thread::sleep(Duration::from_millis(200));
    }
    let residue = unit.destroy(&id).unwrap();
    let _ = std::fs::remove_dir_all(&state);
    assert!(read.contains(BODY), "the payload read {read:?} through an absolute link");
    assert!(residue.is_clean(), "{residue:?}");
}
