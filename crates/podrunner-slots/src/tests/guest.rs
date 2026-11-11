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
    let verdict = self_test(&unit, PIN, "linux/amd64", UnitId::parse("u-1").unwrap(), &dir.0);
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
    let why = self_test(&unit, PIN, "linux/amd64", UnitId::parse("u-2").unwrap(), &dir.0).unwrap_err();
    assert!(why.contains("loopback"), "{why}");
}
