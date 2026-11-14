//! The labels and the run report of podvm (T-078). A unit that exits 3
//! leaves podvm's report of its `run -d`: the digest of its image, no
//! refusal and the label of the unit; the exit code comes from `wait`. A container of the unit carries the label too, and
//! `destroy` removes it. Ignored by default. Run:
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_PODVM_IMAGE=NAME@sha256:HEX \
//!   cargo test -p podrunner-podvm --test report -- --ignored
//!
//! The image needs sh and sleep.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use podrunner_podvm::{Podvm, PodvmUnit};
use podrunner_schema::UnitId;
use podrunner_unit::{ContainerSpec, Exit, StartSpec, UnitDriver};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[test]
#[ignore]
fn the_run_report_names_the_image_the_exit_and_the_label() {
    let state = std::env::temp_dir().join(format!("podrunner-report-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let podvm = PathBuf::from(env("PODRUNNER_PODVM"));
    let unit =
        PodvmUnit::new(Podvm { binary: podvm.clone() }, &state, Path::new(&env("PODRUNNER_PODVM_IMAGES"))).unwrap();
    let image = unit.prepare(&env("PODRUNNER_PODVM_IMAGE"), "linux/amd64", false).unwrap();
    let id = UnitId::parse("u-1").unwrap();
    let spec = StartSpec {
        unit: id.clone(),
        image: image.clone(),
        argv: ["sh", "-c", "sleep 1; exit 3"].map(String::from).to_vec(),
        env: Default::default(),
        credential: Default::default(),
        sockets: None,
        mounts: Vec::new(),
    };
    unit.start(&spec).unwrap();
    // A container of the unit, when the rung has the container route.
    let container = unit
        .container_start(
            &id,
            &ContainerSpec {
                image: env("PODRUNNER_PODVM_IMAGE"),
                argv: ["sleep", "60"].map(String::from).to_vec(),
                ..Default::default()
            },
        )
        .ok();
    let exit = unit.wait(&id, Duration::from_secs(60)).unwrap();
    let mut report = None;
    for _ in 0..50 {
        report = unit.run_report(&id);
        if report.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let store = state.join("units").join("u-1").join("store");
    let labeled = |filter: &str| {
        let out = Command::new(&podvm).args(["ps", "-a", "-q", "--filter", filter]).env("PODVM_STORE", &store).output();
        out.map(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().count()).unwrap_or(0)
    };
    let before = labeled("label=podrunner.unit=u-1");
    let residue = unit.destroy(&id).unwrap();
    let _ = std::fs::remove_dir_all(&state);
    assert_eq!(exit, Some(Exit::Code(3)));
    let r = report.expect("podvm wrote no report of the run");
    assert_eq!(r["image_digest"], image.digest.as_str(), "{r}");
    // The report of `run -d` ends with the start: the exit comes from
    // `podvm wait`.
    assert_eq!((r["detached"].as_bool(), r["refused"].is_null()), (Some(true), true), "{r}");
    assert!(r["labels"].to_string().contains("podrunner.unit"), "{r}");
    // The unit and its container carry the label.
    assert_eq!(before, 1 + usize::from(container.is_some()), "labeled containers");
    assert!(residue.is_clean(), "{residue:?}");
}
