//! #257: a destroy of a unit with no record (a start that failed after
//! `podvm run -d`, or a replay of the journal) still removes the podvm unit.

use std::os::unix::fs::PermissionsExt;

use podrunner_schema::UnitId;
use podrunner_unit::UnitDriver;

use super::PodvmUnit;

#[test]
fn a_destroy_with_no_record_removes_the_podvm_unit() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let d = std::env::temp_dir().join(format!("podrunner-podvm-norecord-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let log = d.join("calls");
    let podvm = d.join("podvm");
    // The version of podvm for `new`, and a line of each call.
    let body = format!(
        "#!/bin/sh\necho \"$*\" >> {}\necho '{{\"podvm\":\"0.2.0-beta.21\",\"rung\":\"userland\"}}'\n",
        log.display()
    );
    std::fs::write(&podvm, body).unwrap();
    std::fs::set_permissions(&podvm, std::fs::Permissions::from_mode(0o755)).unwrap();
    let unit = PodvmUnit::new(crate::Podvm { binary: podvm }, &d.join("state"), &d).unwrap();
    std::fs::create_dir_all(d.join("state/units/u-3/store")).unwrap();
    let residue = unit.destroy(&UnitId::parse("u-3").unwrap()).unwrap();
    let calls = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(calls.lines().any(|l| l.ends_with("rm -f u-3")), "{calls}");
    assert!(!d.join("state/units/u-3").exists(), "{residue:?}");
    let _ = std::fs::remove_dir_all(&d);
}
