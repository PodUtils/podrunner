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
    std::fs::create_dir_all(d.join("state/units/u-3")).unwrap();
    std::fs::create_dir_all(d.join("state/store")).unwrap();
    let residue = unit.destroy(&UnitId::parse("u-3").unwrap()).unwrap();
    let calls = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(calls.lines().any(|l| l.ends_with("rm -f u-3")), "{calls}");
    assert!(!d.join("state/units/u-3").exists(), "{residue:?}");
    let _ = std::fs::remove_dir_all(&d);
}

/// A fake podvm in `d`: the launcher of each unit is `dead` for `inspect`,
/// and `wait` prints `wait`.
fn dead_launcher_fake(d: &std::path::Path, wait: &str) -> std::path::PathBuf {
    let body = format!(
        r#"#!/bin/sh
S=$PODVM_STORE
case "$1" in
probe) echo '{{"podvm":"0.2.0-beta.40","rung":"userland"}}' ;;
load) echo "runner@{MANIFEST}" >> "$S/images.txt" ;;
images) [ "$2" = --format ] && cat "$S/images.txt" 2>/dev/null ;;
extract) mkdir -p "$S/rootfs/img/rootfs/etc"; echo "$S/rootfs/img/rootfs" ;;
run) while [ $# -gt 0 ]; do [ "$1" = --name ] && n=$2; shift; done
  mkdir -p "$S/trees/$n"; cp -R "$S/rootfs/img/rootfs" "$S/trees/$n/rootfs" ;;
inspect)
  if [ "$2" = --format ]; then n=$4; [ -d "$S/trees/$n" ] || exit 1; echo "$S/trees/$n/rootfs"
  else echo '[{{"State":{{"Status":"dead","Dead":true,"ExitCode":null}}}}]'; fi ;;
wait) echo '{wait}' ;;
rm) rm -rf "$S/trees/$3" ;;
esac
exit 0
"#
    );
    let bin = d.join("podvm");
    std::fs::write(&bin, body).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

const PIN: &str = "sha256:3333333333333333333333333333333333333333333333333333333333333333";
const MANIFEST: &str = "sha256:4444444444444444444444444444444444444444444444444444444444444444";

/// The residue of a unit whose launcher podvm names `dead`, after the
/// payload ended as `wait` says, with no process that carries the tag.
fn residue_after_dead_launcher(name: &str, wait: &str) -> podrunner_unit::Residue {
    use podrunner_unit::{Prepared, StartSpec};
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let d = std::env::temp_dir().join(format!("podrunner-podvm-dead-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("state/images")).unwrap();
    super::store_tests::write_tar(&d.join(format!("state/images/{}.tar", PIN.trim_start_matches("sha256:"))), MANIFEST);
    let unit = PodvmUnit::new(crate::Podvm { binary: dead_launcher_fake(&d, wait) }, &d.join("state"), &d).unwrap();
    let id = UnitId::parse("u-5").unwrap();
    let spec = StartSpec {
        unit: id.clone(),
        image: Prepared { digest: PIN.into(), platform: "linux/amd64".into() },
        argv: vec!["true".into()],
        env: Default::default(),
        credential: podrunner_unit::conformance::canary_credential(),
        sockets: None,
        mounts: Vec::new(),
        limits: Default::default(),
    };
    unit.start(&spec).unwrap();
    // The waiter is an event: `wait` returns when the fake has ended.
    let exit = unit.wait(&id, std::time::Duration::from_secs(30)).unwrap();
    assert!(exit.is_some(), "the fake wait did not end");
    let residue = unit.destroy(&id).unwrap();
    let _ = std::fs::remove_dir_all(&d);
    residue
}

/// T-696: after `job-mismatch` the hook ends the worker, the runner exits
/// 0 and podvm names the launcher `dead`. No process carries the tag: the
/// count is 0, not `Unknown`, so the unit is not `degraded`.
#[test]
fn a_dead_launcher_after_an_exit_with_no_tagged_process_leaves_none() {
    let residue = residue_after_dead_launcher("exit", "0");
    assert_eq!(residue.processes, podrunner_unit::Count::Known(0), "{residue:?}");
    assert!(residue.is_clean(), "{residue:?}");
}

/// A dead launcher and no exit of the payload (design test 15, a killed
/// launcher): the orphans can be out of the tag, so the count stays
/// `Unknown`.
#[test]
fn a_dead_launcher_with_no_exit_has_no_count() {
    let residue = residue_after_dead_launcher("noexit", "");
    assert_eq!(residue.processes, podrunner_unit::Count::Unknown, "{residue:?}");
}
