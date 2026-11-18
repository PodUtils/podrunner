//! T-506 (githome's G-36): a sandbox offer runs its command in a unit of
//! its pinned image, at `untrusted`, and a runner job never takes that pin.

use podrunner_schema::{Boundary, EgressHold, JobOffer, Principal, RefusalCode, Tier};
use serde_json::json;

use super::*;
use crate::placement::{Kind, place};

const SANDBOX: &str = "agent@sha256:2222222222222222222222222222222222222222222222222222222222222222";

fn job(with: serde_json::Value) -> JobOffer {
    let line: Value = serde_json::from_str(&offer(0, "j-1", with)).unwrap();
    serde_json::from_value(line["offer"].clone()).unwrap()
}

fn kind(c: Capability) -> Kind {
    Kind { capability: c, free: 1, running: 0, repos: vec![] }
}

/// The configuration with the image of a sandbox pinned before the runner.
fn cfg() -> Config {
    config_pin(
        "",
        &format!(
            "[[pin.payload]]\narch = \"x86_64\"\nreference = \"{SANDBOX}\"\nbuilt_at_s = {NOW_UNIX}\nsandbox = true"
        ),
    )
}

fn sandbox(image: &str) -> serde_json::Value {
    json!({ "sandbox": { "image": image, "command": ["agent", "--serve"], "idle_s": 600, "max_s": 3600 } })
}

#[test]
fn a_sandbox_runs_its_pinned_image_at_untrusted() {
    let guest = capability(Boundary::Guest, Principal::Distinct, EgressHold::Enforced);
    let confined = capability(Boundary::Confined, Principal::Distinct, EgressHold::Enforced);
    // A member on a trusted set: the sandbox still runs at untrusted.
    let p =
        place(&job(sandbox(SANDBOX)), Tier::Trusted, &[kind(confined.clone()), kind(guest)], &cfg(), NOW_UNIX).unwrap();
    assert_eq!((p.tier, p.kind, p.payload.as_str()), (Tier::Untrusted, 1, SANDBOX));
    // No guest: refused, as an untrusted job is.
    assert!(place(&job(sandbox(SANDBOX)), Tier::Trusted, &[kind(confined.clone())], &cfg(), NOW_UNIX).is_err());
    // An image that the operator did not pin as a sandbox.
    let other = format!("agent@sha256:{}", "3".repeat(64));
    let guest = capability(Boundary::Guest, Principal::Distinct, EgressHold::Enforced);
    assert_eq!(
        place(&job(sandbox(&other)), Tier::Trusted, &[kind(guest.clone())], &cfg(), NOW_UNIX).map(|p| p.payload),
        Err(RefusalCode::PayloadNotPinned)
    );
    // The runner pin is no sandbox image.
    assert_eq!(
        place(&job(sandbox(PIN)), Tier::Trusted, &[kind(guest)], &cfg(), NOW_UNIX).map(|p| p.payload),
        Err(RefusalCode::PayloadNotPinned)
    );
    // A runner job never takes the pin of a sandbox, also when it comes first.
    let runner = place(&job(json!({})), Tier::Trusted, &[kind(confined)], &cfg(), NOW_UNIX).unwrap();
    assert_eq!(runner.payload, PIN);
}

#[test]
fn a_sandbox_gets_its_command_its_idle_time_and_its_end() {
    let o = job(sandbox(SANDBOX));
    let kind_cmd = vec!["./run.sh".to_string()];
    assert_eq!(crate::sandbox::argv(Some(&o), &kind_cmd), ["agent", "--serve"]);
    assert_eq!(crate::sandbox::argv(Some(&job(json!({}))), &kind_cmd), kind_cmd);
    let mut env = std::collections::BTreeMap::new();
    crate::sandbox::env(Some(&o), &mut env);
    assert_eq!(env.get(crate::sandbox::IDLE_VAR).map(String::as_str), Some("600"));
    assert_eq!(crate::sandbox::wall_s(Some(&o), 7200), 3600);
    assert_eq!(crate::sandbox::wall_s(Some(&o), 60), 60);
    assert_eq!(crate::sandbox::wall_s(Some(&job(json!({}))), 7200), 7200);
}

#[test]
fn a_sandbox_runs_its_command_in_a_unit_and_ends() {
    let guest = capability(Boundary::Guest, Principal::Distinct, EgressHold::Enforced);
    let with = json!({ "sandbox": {
        "image": SANDBOX, "command": ["sh", "-c", "test \"$PODRUNNER_IDLE_S\" = 600 || exit 5; exit 0"], "idle_s": 600
    }});
    let mut bed = Bed::new(
        cfg(),
        vec![("a", "untrusted", None, vec![offer(0, "j-1", with)])],
        // The command of the kind (the runner) fails: the sandbox does not run it.
        vec![KindSpec { name: "fake", slots: 1, capability: guest, command: sh("exit 9") }],
    );
    bed.until(60, |s| s.jobs().count() == 1 && Bed::all_final(s));
    let r = &bed.slots.reports[0];
    assert_eq!(r.exit_reason, podrunner_schema::ExitReason::Success, "{r:?}");
    assert_eq!(r.unit.as_ref().map(|u| u.boundary), Some(Boundary::Guest), "{r:?}");
}
