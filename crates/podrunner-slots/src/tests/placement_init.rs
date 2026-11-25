//! T-549: the labels of a job choose its payload, and a payload that boots
//! the init of its image (systemd) goes only to a kind that boots it. A job
//! of `ubuntu-latest` with the hosted snapshot never goes to the podvm kind.

use podrunner_schema::{Boundary, EgressHold, JobOffer, Principal, RefusalCode, Tier};
use serde_json::json;

use super::*;
use crate::config::{Payload, UnitInit};
use crate::placement::{Kind, place, place_why};

const SNAPSHOT: &str = "podrunner-hosted@sha256:2222222222222222222222222222222222222222222222222222222222222222";

fn job(labels: &[&str]) -> JobOffer {
    let line: Value = serde_json::from_str(&offer(0, "j-1", json!({ "labels": labels }))).unwrap();
    serde_json::from_value(line["offer"].clone()).unwrap()
}

fn kind(init: Option<UnitInit>) -> Kind {
    let c = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    Kind {
        engine_needs_payload: false,
        capability: c,
        free: 1,
        running: 0,
        repos: vec![],
        root_disk: None,
        init,
        warm: vec![],
    }
}

/// The runner payload of the tests (no labels, no init) and the snapshot,
/// which serves `ubuntu-latest` and boots its own init.
fn hosted() -> Config {
    let mut cfg = config("");
    let runner = cfg.pin.payload[0].clone();
    cfg.pin.payload.insert(
        0,
        Payload {
            reference: SNAPSHOT.into(),
            labels: vec!["ubuntu-latest".into(), "ubuntu-24.04".into()],
            init: Some(UnitInit::Image),
            ..runner
        },
    );
    cfg
}

#[test]
fn a_systemd_payload_never_goes_to_a_kind_with_no_init() {
    let cfg = hosted();
    let podvm = kind(None);
    let incus = kind(Some(UnitInit::Image));
    let at = |labels: &[&str], kinds: &[Kind]| {
        place(&job(labels), Tier::Trusted, kinds, &cfg, NOW_UNIX).map(|p| (p.kind, p.payload))
    };
    // The podvm kind is first and free: the snapshot still goes to incus.
    assert_eq!(at(&["ubuntu-latest"], &[podvm.clone(), incus.clone()]), Ok((1, SNAPSHOT.into())));
    // No case: GitHub compares a label with no case.
    assert_eq!(at(&["Ubuntu-Latest"], &[podvm.clone(), incus.clone()]), Ok((1, SNAPSHOT.into())));
    assert_eq!(at(&["ubuntu-latest"], std::slice::from_ref(&podvm)), Err(RefusalCode::BoundaryNotAvailable));
    // A job of another label takes the runner payload, on the podvm kind
    // only: the init of the image would boot an image with no init.
    assert_eq!(at(&["self-hosted"], &[incus.clone(), podvm.clone()]), Ok((1, PIN.into())));
    assert_eq!(at(&["self-hosted"], std::slice::from_ref(&incus)), Err(RefusalCode::BoundaryNotAvailable));
}

/// The refusal names the payload and its init, for the audit line.
#[test]
fn the_refusal_names_the_payload_and_its_init() {
    let cfg = hosted();
    let r = place_why(&job(&["ubuntu-24.04"]), Tier::Trusted, &[kind(None)], &cfg, &cfg.trust, NOW_UNIX).unwrap_err();
    assert_eq!(r.code, RefusalCode::BoundaryNotAvailable);
    let detail = r.detail.unwrap_or_default();
    assert!(detail.contains(&format!("the payload {SNAPSHOT} needs init = \"image\"")), "{detail}");
}

/// A payload that names labels serves only them; with no payload for the
/// labels of a job and none with no labels, the job is `payload-not-pinned`.
#[test]
fn a_payload_with_labels_serves_only_its_labels() {
    let mut cfg = hosted();
    cfg.pin.payload.retain(|p| p.reference == SNAPSHOT);
    let incus = [kind(Some(UnitInit::Image))];
    let at = |labels: &[&str]| place(&job(labels), Tier::Trusted, &incus, &cfg, NOW_UNIX).map(|p| p.payload);
    assert_eq!(at(&["ubuntu-latest"]), Ok(SNAPSHOT.into()));
    assert_eq!(at(&["podrunner"]), Err(RefusalCode::PayloadNotPinned));
}

/// Through the slots: the kind takes its `init` from `[unit.KIND] init`,
/// and the audit line of the refusal says why.
#[test]
fn the_slots_refuse_a_systemd_payload_with_an_audit_line() {
    let run = |init: Option<UnitInit>| {
        let lines = vec![offer(0, "j-1", json!({ "labels": ["ubuntu-latest"] }))];
        let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
        let mut bed = Bed::new(
            hosted(),
            vec![("a", "trusted", None, lines)],
            vec![KindSpec { name: "fake", slots: 1, capability: cap, command: sh("exit 0") }],
        );
        bed.slots.kinds[0].cfg.init = init;
        bed.until(20, |s| s.jobs().count() == 1 && Bed::all_final(s));
        bed
    };
    let bed = run(None);
    assert_eq!(bed.slots.reports[0].refusal_code, Some(RefusalCode::BoundaryNotAvailable));
    let refuse = bed.slots.audit_lines().iter().find(|a| a.audit == crate::audit::Kind::Refuse).unwrap();
    let detail = refuse.detail.as_deref().unwrap_or_default();
    assert!(detail.contains("init = \"image\""), "{detail}");
    let bed = run(Some(UnitInit::Image));
    assert_eq!(bed.slots.reports[0].refusal_code, None, "{:?}", bed.slots.reports[0]);
}

/// The file: only the incus and the microvm kinds boot an init, a payload
/// that needs one needs such a kind, and a label is one word.
#[test]
fn the_file_holds_the_init_of_the_kinds_and_of_the_pins() {
    let parse = |units: &str, pin: &str| {
        Config::parse(&format!(
            "host = \"h\"\n{units}\n[pin]\n[[pin.payload]]\narch = \"x86_64\"\nreference = \"{SNAPSHOT}\"\n\
             built_at_s = 1\n{pin}\n"
        ))
    };
    let image = "init = \"image\"\nlabels = [\"ubuntu-latest\"]";
    assert!(parse("[unit.incus]\nslots = 1\ninit = \"image\"", image).is_ok());
    assert!(parse("[unit.microvm]\nslots = 1\ninit = \"image\"", image).is_ok());
    let e = parse("[unit.podvm]\nslots = 1\ninit = \"image\"", "").unwrap_err();
    assert!(e.contains("[unit.podvm] init: only the incus and the microvm kinds"), "{e}");
    let e = parse("[unit.podvm]\nslots = 1", image).unwrap_err();
    assert!(e.contains("no [unit.KIND] boots it"), "{e}");
    let e = parse("[unit.incus]\nslots = 1\ninit = \"image\"", "init = \"image\"\nlabels = [\"a b\"]").unwrap_err();
    assert!(e.contains("label \"a b\" is empty or has white space"), "{e}");
    assert!(parse("", "init = \"systemd\"").is_err(), "one value: image");
}

#[test]
fn a_guest_container_route_requires_the_selected_payload_engine_or_a_host_override() {
    let mut cfg = config("");
    let full = format!("runner-full@sha256:{}", "2".repeat(64));
    cfg.pin.payload.push(Payload {
        reference: full.clone(),
        labels: vec!["docker".into()],
        container_engine: true,
        ..cfg.pin.payload[0].clone()
    });
    let mut guest = kind(None);
    guest.capability.container = podrunner_schema::ContainerRoute::Engine;
    guest.engine_needs_payload = true;
    let place_on = |labels: &[&str], need, k: &Kind| {
        let mut j = job(labels);
        j.unit_need = serde_json::from_value(need).unwrap();
        place(&j, Tier::Trusted, std::slice::from_ref(k), &cfg, NOW_UNIX).map(|p| p.payload)
    };
    assert_eq!(place_on(&[], json!(null), &guest), Ok(PIN.into()));
    for need in [json!({"container": true}), json!({"container_engine": true})] {
        assert_eq!(place_on(&[], need.clone(), &guest), Err(RefusalCode::ContainerNotSupported));
        assert_eq!(place_on(&["docker"], need.clone(), &guest), Ok(full.clone()));
        let mut overridden = guest.clone();
        overridden.engine_needs_payload = false;
        assert_eq!(place_on(&[], need, &overridden), Ok(PIN.into()));
    }
}
