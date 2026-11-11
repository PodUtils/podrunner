//! A fake unit that claims `guest` runs on the kernel of the host: the
//! self-test fails, and its claim drops to `none`.

use podrunner_fake::FakeUnit;
use podrunner_schema::{Boundary, EgressHold, Principal, UnitId};

use super::*;
use crate::guest::{self_test, settle};

#[test]
fn a_fake_that_claims_guest_fails_the_self_test() {
    let dir = TempDir::new();
    let claim = capability(Boundary::Guest, Principal::Distinct, EgressHold::Enforced);
    let unit = FakeUnit::new(&dir.0.join("fake"), claim.clone(), "linux/amd64");
    let verdict =
        self_test(&unit, PIN, "linux/amd64", (UnitId::parse("u-10").unwrap(), UnitId::parse("u-1").unwrap()), &dir.0);
    let why = verdict.clone().unwrap_err();
    assert!(why.contains("boot id") || why.contains("loopback"), "{why}");
    let (cap, note) = settle(claim.clone(), true, verdict);
    assert_eq!(cap.boundary, Boundary::None);
    assert_eq!(cap.boundary_proof.as_deref(), Some("guest-self-test-failed"));
    assert!(note.is_some());
    // A kind not on the allowlist drops too, with no test.
    let (cap, _) = settle(claim, false, Ok(()));
    assert_eq!(cap.boundary, Boundary::None);
}

#[test]
fn a_kind_below_guest_is_not_touched() {
    let c = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    assert_eq!(settle(c.clone(), false, Err("x".into())), (c, None));
}

/// A unit on the network of the host reaches the planted listener, and
/// that alone fails the claim.
#[test]
fn the_loopback_listener_is_reached_from_the_host_network() {
    let has_tool = std::process::Command::new("sh")
        .args(["-c", "command -v nc || command -v bash"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !has_tool {
        return; // No tool to connect with on this host.
    }
    let dir = TempDir::new();
    let claim = capability(Boundary::Guest, Principal::Distinct, EgressHold::Enforced);
    let unit = FakeUnit::new(&dir.0.join("fake"), claim, "linux/amd64");
    let why =
        self_test(&unit, PIN, "linux/amd64", (UnitId::parse("u-20").unwrap(), UnitId::parse("u-2").unwrap()), &dir.0)
            .unwrap_err();
    assert!(why.contains("loopback"), "{why}");
}

/// A second fake unit sees the process and the file of the first: the
/// lines say so.
#[test]
fn a_second_fake_unit_sees_the_first() {
    let dir = TempDir::new();
    let claim = capability(Boundary::Guest, Principal::Distinct, EgressHold::Enforced);
    let unit = FakeUnit::new(&dir.0.join("fake"), claim, "linux/amd64");
    let ids = (UnitId::parse("u-30").unwrap(), UnitId::parse("u-3").unwrap());
    let (text, _) = crate::guest::observe(&unit, PIN, "linux/amd64", ids, &dir.0).unwrap();
    assert!(text.contains("firstproc=visible"), "{text}");
    assert!(text.contains("firstfile=visible"), "{text}");
    assert!(text.contains("hostpid=visible") || text.contains("hostpid=absent"), "{text}");
}

/// The claim of `guest` is tested again when the driver file changes and
/// after a day; a fake fails the test, so its claim drops to `none`.
#[test]
fn the_self_test_runs_again_on_a_driver_change_and_each_day() {
    use crate::guest::{Retest, stamp};
    let claim = capability(Boundary::Guest, Principal::Distinct, EgressHold::Enforced);
    let bed = |driver: Option<std::path::PathBuf>| {
        let mut bed = Bed::new(
            config(""),
            vec![],
            vec![KindSpec { name: "fake", slots: 1, capability: claim.clone(), command: sh("exit 0") }],
        );
        let now = bed.slots.now();
        let stamp = driver.as_deref().and_then(stamp);
        let r = Retest {
            claim: claim.clone(),
            allowed: true,
            payload: PIN.into(),
            platform: "linux/amd64".into(),
            last_ms: now,
            driver,
            stamp,
        };
        bed.slots.retest_guest(0, r);
        bed
    };
    let boundary = |b: &Bed| b.slots.status().kinds[0].probe.boundary;
    // A change of the driver file.
    let dir = TempDir::new();
    let driver = dir.0.join("driver");
    std::fs::write(&driver, "v1").unwrap();
    let mut b = bed(Some(driver.clone()));
    b.tick();
    assert_eq!(boundary(&b), Boundary::Guest);
    std::fs::write(&driver, "version 2").unwrap();
    b.tick();
    assert_eq!(boundary(&b), Boundary::None);
    assert_eq!(b.slots.status().kinds[0].probe.boundary_proof.as_deref(), Some("guest-self-test-failed"));
    // A day.
    let mut b = bed(None);
    b.tick();
    assert_eq!(boundary(&b), Boundary::Guest);
    b.clock.advance(std::time::Duration::from_millis(crate::guest::RETEST_MS));
    b.tick();
    assert_eq!(boundary(&b), Boundary::None);
}

/// A driver comes from an absolute path, in `driver_dir` when it is set,
/// and the file and its directory are not writable by others.
#[test]
fn a_driver_comes_from_a_checked_place() {
    use std::os::unix::fs::PermissionsExt;
    let unit = |bin: &str, dir: &str| {
        crate::config::Config::parse(&format!(
            "host = \"h\"\n{dir}\n[unit.podvm]\nslots = 1\nbinary = \"{bin}\"\n[pin]\n"
        ))
    };
    assert!(unit("podvm", "").unwrap_err().contains("never comes from PATH"));
    assert!(unit("/opt/podvm/podvm", "driver_dir = \"/opt/drivers\"").unwrap_err().contains("driver_dir"));
    assert!(unit("/opt/drivers/podvm", "driver_dir = \"/opt/drivers\"").is_ok());
    let tmp = TempDir::new();
    let dir = tmp.0.join("drivers");
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let file = dir.join("podvm");
    std::fs::write(&file, "").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(crate::config::check_driver(&file), Ok(()));
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o775)).unwrap();
    assert!(crate::config::check_driver(&file).unwrap_err().contains("writable"));
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(crate::config::check_driver(&file).unwrap_err().contains("writable"));
    assert!(crate::config::check_driver(&dir.join("missing")).is_err());
}
