//! The guards of placement and of the life of a unit, each with the input
//! that the guard must refuse.

use podrunner_schema::{Boundary, EgressHold, ExitReason, JobOffer, Principal, RefusalCode, Tier, TreeKill};
use serde_json::json;

use super::*;
use crate::placement::{Kind, place};

fn job(with: serde_json::Value) -> JobOffer {
    let line: Value = serde_json::from_str(&offer(0, "j-1", with)).unwrap();
    serde_json::from_value(line["offer"].clone()).unwrap()
}

fn kind(c: Capability) -> Kind {
    Kind { capability: c, free: 1, running: 0, repos: vec![] }
}

fn confined() -> Capability {
    capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory)
}

fn fork() -> Value {
    json!({ "trust_inputs": { "event": "pull_request", "head_is_fork": "yes", "actor_class": "outside", "repo_visibility": "public" } })
}

#[test]
fn untrusted_never_runs_below_guest() {
    let cfg = config("");
    assert_eq!(
        place(&job(fork()), Tier::Untrusted, &[kind(confined())], &cfg, NOW_UNIX),
        Err(RefusalCode::TierNeedsGuest)
    );
    // An offer cannot lower the floor of its tier.
    let low = job(json!({ "trust_inputs": fork()["trust_inputs"], "unit_need": { "min_boundary": "none" } }));
    assert_eq!(place(&low, Tier::Untrusted, &[kind(confined())], &cfg, NOW_UNIX), Err(RefusalCode::TierNeedsGuest));
    // The configuration cannot lower it either.
    for floor in ["redirect", "confined"] {
        let e =
            Config::parse(&format!("host = \"h\"\n[tier.untrusted]\nmin_boundary = \"{floor}\"\n[pin]\n")).unwrap_err();
        assert!(e.contains("below the floor"), "{e}");
    }
    let e = Config::parse("host = \"h\"\n[tier.trusted]\nmin_boundary = \"none\"\n[pin]\n").unwrap_err();
    assert!(e.contains("below the floor"), "{e}");
    // A control: a guest kind with an enforced hold takes the job.
    let guest = capability(Boundary::Guest, Principal::Distinct, EgressHold::Enforced);
    assert_eq!(
        place(&job(fork()), Tier::Untrusted, &[kind(guest)], &cfg, NOW_UNIX).map(|p| p.tier),
        Ok(Tier::Untrusted)
    );
}

/// DR-4: `trusted` at `redirect` only when the file sets it, and only with
/// one job at a time on a shared uid and a slot cap on each trusted source.
#[test]
fn trusted_at_redirect_needs_the_mitigations_of_dr4() {
    let redirect = capability(Boundary::Redirect, Principal::Shared, EgressHold::Advisory);
    let member = job(json!({}));
    // The default floor stays `confined`.
    assert_eq!(config("").floor(Tier::Trusted), Boundary::Confined);
    assert!(place(&member, Tier::Trusted, &[kind(redirect.clone())], &config(""), NOW_UNIX).is_err());
    let floor = "[tier.trusted]\nmin_boundary = \"redirect\"";
    let cfg = config_signed(&format!("repository = \"example-org/app\"\n{floor}"));
    assert_eq!(place(&member, Tier::Trusted, &[kind(redirect)], &cfg, NOW_UNIX).map(|p| p.tier), Ok(Tier::Trusted));
    let (k, s) = crate::pins::sign(&Config::parse("host = \"h\"\n[pin]\n").unwrap().pin, &[1; 32]);
    let signed = format!("[pin]\nsigner = \"{k}\"\nsignature = \"{s}\"");
    let unsigned = Config::parse(&format!("host = \"h\"\nrepository = \"o/r\"\n{floor}\n[pin]\n")).unwrap_err();
    assert!(unsigned.contains("signed pin list"), "{unsigned}");
    let parse = |extra: &str| Config::parse(&format!("host = \"h\"\nallow_fake = true\n{extra}\n{floor}\n{signed}\n"));
    let repo = "repository = \"example-org/app\"";
    let e = parse("").unwrap_err();
    assert!(e.contains("one repository"), "{e}");
    let e = parse(&format!("{repo}\nsame_principal_ok = true")).unwrap_err();
    assert!(e.contains("same_principal_ok = false"), "{e}");
    let source = "[[source]]\nname = \"s\"\nkind = \"fake\"\ntier = \"trusted\"\narch = \"x86_64\"";
    let e = parse(&format!("{repo}\n{source}")).unwrap_err();
    assert!(e.contains("needs slots"), "{e}");
    assert!(parse(&format!("{repo}\n{source}\nslots = 1")).is_ok());
    assert!(parse("repository = \"no-slash\"").unwrap_err().contains("owner/repo"));
}

/// DR-4: a host with `repository` refuses an offer of another repository,
/// terminal, before any other step.
#[test]
fn a_host_of_one_repository_refuses_another() {
    let cfg = config("repository = \"example-org/other\"");
    let other = place(&job(json!({})), Tier::Trusted, &[kind(confined())], &cfg, NOW_UNIX);
    assert_eq!(other, Err(RefusalCode::RepoScopeRefused));
    assert!(!RefusalCode::RepoScopeRefused.is_wait());
    // A control: the same offer on a host of its repository runs.
    let cfg = config("repository = \"Example-Org/App\"");
    assert!(place(&job(json!({})), Tier::Trusted, &[kind(confined())], &cfg, NOW_UNIX).is_ok());
}

#[test]
fn a_fork_offer_on_the_trusted_set_is_refused() {
    assert_eq!(
        place(&job(fork()), Tier::Trusted, &[kind(confined())], &config(""), NOW_UNIX),
        Err(RefusalCode::TierMismatch)
    );
}

/// The design test 2 of the source contract: an offer that comes twice
/// is one job, with one offer line in the audit.
#[test]
fn a_redelivered_offer_is_one_job() {
    let lines = vec![offer(0, "j-1", json!({})), offer(1, "j-1", json!({}))];
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, lines)],
        vec![KindSpec { name: "fake", slots: 1, capability: confined(), command: sh("sleep 3") }],
    );
    bed.until(5, |s| s.jobs().any(|j| j.state == State::Started));
    bed.tick();
    bed.tick();
    assert_eq!(bed.slots.jobs().count(), 1);
    let offers = bed.slots.audit_lines().iter().filter(|a| a.audit == crate::audit::Kind::Offer).count();
    assert_eq!(offers, 1);
    bed.until(400, Bed::all_final);
}

/// The design test 11 of the source contract: a payload in an offer of a
/// `github-scaleset` source fails the offer check (refused at intake), and
/// a payload that a set does not take is ignored with an audit line.
#[test]
fn a_payload_of_a_scaleset_is_refused_and_an_ignored_one_is_audited() {
    let other = format!("sha256:{}", "2".repeat(64));
    let with_payload = job(json!({ "payload": other }));
    assert!(with_payload.check(true).is_err());
    assert!(with_payload.check(false).is_ok());
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({ "payload": other }))])],
        vec![KindSpec { name: "fake", slots: 1, capability: confined(), command: sh("exit 0") }],
    );
    bed.until(40, |s| s.jobs().count() == 1 && Bed::all_final(s));
    assert!(bed.slots.audit_lines().iter().any(|a| a.audit == crate::audit::Kind::PayloadIgnored));
}

#[test]
fn a_stale_image_and_a_digest_off_the_pin_list_are_refused() {
    let cfg = config("");
    let k = [kind(confined())];
    let month = 31 * 24 * 3600;
    assert_eq!(place(&job(json!({})), Tier::Trusted, &k, &cfg, NOW_UNIX + month), Err(RefusalCode::ImageStale));
    let other = format!("sha256:{}", "2".repeat(64));
    let with_payload = job(json!({ "payload": other }));
    // With no payload_from_offer, the payload of the offer is ignored.
    let p = place(&with_payload, Tier::Trusted, &k, &cfg, NOW_UNIX).unwrap();
    assert!(p.payload_ignored);
    assert_eq!(p.payload, PIN);
    let from_offer = config("payload_from_offer = true");
    assert_eq!(place(&with_payload, Tier::Trusted, &k, &from_offer, NOW_UNIX), Err(RefusalCode::PayloadNotPinned));
    let pinned = job(json!({ "payload": format!("sha256:{}", "1".repeat(64)) }));
    assert!(!place(&pinned, Tier::Trusted, &k, &from_offer, NOW_UNIX).unwrap().payload_ignored);
}

#[test]
fn a_required_limit_that_no_kind_enforces_is_refused() {
    let wall = job(json!({ "limits": { "wall": 60 }, "limits_required": ["wall"] }));
    let unverified = Capability { tree_kill: TreeKill::Unverified, ..confined() };
    assert_eq!(
        place(&wall, Tier::Trusted, &[kind(unverified)], &config(""), NOW_UNIX),
        Err(RefusalCode::LimitNotEnforceable)
    );
    assert!(place(&wall, Tier::Trusted, &[kind(confined())], &config(""), NOW_UNIX).is_ok());
}

#[test]
fn a_shared_principal_runs_one_unit_at_a_time() {
    let shared = capability(Boundary::Confined, Principal::Shared, EgressHold::Advisory);
    let busy = Kind { capability: shared.clone(), free: 1, running: 1, repos: vec!["example-org/app".into()] };
    assert_eq!(
        place(&job(json!({})), Tier::Trusted, std::slice::from_ref(&busy), &config(""), NOW_UNIX),
        Err(RefusalCode::NoSlotOfKind)
    );
    let ok = config("same_principal_ok = true");
    assert!(place(&job(json!({})), Tier::Trusted, std::slice::from_ref(&busy), &ok, NOW_UNIX).is_ok());
    let other_repo = job(json!({ "trust_inputs": { "repo": "other" } }));
    assert_eq!(place(&other_repo, Tier::Trusted, &[busy], &ok, NOW_UNIX), Err(RefusalCode::NoSlotOfKind));
}

#[test]
fn a_foreign_runner_name_ends_the_unit() {
    let lines = vec![
        offer(0, "j-1", json!({})),
        r#"{"at_s":3,"started":{"job_id":"j-1","runner_name":"someone-else"}}"#.to_string(),
    ];
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, lines)],
        vec![KindSpec { name: "fake", slots: 1, capability: confined(), command: sh("sleep 300") }],
    );
    bed.until(40, |s| s.jobs().count() == 1 && Bed::all_final(s));
    let r = &bed.slots.reports[0];
    assert_eq!(r.exit_reason, ExitReason::JobMismatch);
    assert!(r.unit.as_ref().unwrap().residual_processes == podrunner_schema::Residual::Count(0));
    assert!(bed.slots.audit_lines().iter().any(|a| a.audit == crate::audit::Kind::JobMismatch));
}

/// The design test 10 of the source contract: a `started` that names the
/// runner of this unit with another job id ends the unit.
#[test]
fn a_started_with_another_job_id_ends_the_unit() {
    let lines = vec![
        offer(0, "j-1", json!({})),
        r#"{"at_s":3,"started":{"job_id":"j-9","runner_name":"podrunner-host-a-u-0"}}"#.to_string(),
    ];
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, lines)],
        vec![KindSpec { name: "fake", slots: 1, capability: confined(), command: sh("sleep 300") }],
    );
    bed.until(40, |s| s.jobs().count() == 1 && Bed::all_final(s));
    let r = &bed.slots.reports[0];
    assert_eq!(r.exit_reason, ExitReason::JobMismatch);
    assert!(bed.slots.audit_lines().iter().any(|a| a.audit == crate::audit::Kind::JobMismatch));
}

#[test]
fn the_right_runner_name_moves_the_job_to_running() {
    let lines = vec![
        offer(0, "j-1", json!({})),
        r#"{"at_s":3,"started":{"job_id":"j-1","runner_name":"podrunner-host-a-u-0"}}"#.to_string(),
    ];
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, lines)],
        vec![KindSpec { name: "fake", slots: 1, capability: confined(), command: sh("sleep 1") }],
    );
    bed.until(10, |s| s.jobs().any(|j| j.state == State::Running));
    bed.until(400, Bed::all_final);
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success);
}

/// A kill of podrunner leaves a unit; the restart destroys it, releases the
/// job and reports it `lost`. With no replay, the unit would live on.
#[test]
fn a_restart_ends_the_units_of_the_journal() {
    let caps = || {
        vec![KindSpec { name: "fake", slots: 1, capability: confined(), command: sh("setsid sleep 300 & sleep 300") }]
    };
    let mut bed = Bed::new(config(""), vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])], caps());
    bed.until(10, |s| s.jobs().any(|j| j.state == State::Started));
    let tag = podrunner_fake::tag_of(&bed.dir.0.join("fake"), "u-0");
    assert!(until_ms(|| podrunner_fake::tagged(&tag).len() >= 2), "the unit runs");
    let Bed { slots, clock, dir } = bed;
    // The kill of podrunner: no drop runs, so the fake kills nothing.
    std::mem::forget(slots);
    assert!(podrunner_fake::tagged(&tag).len() >= 2, "the processes outlive podrunner");
    let bed = Bed::in_dir(config(""), vec![("a", "trusted", None, vec![])], caps(), dir, clock);
    assert!(podrunner_fake::tagged(&tag).is_empty(), "the restart killed the unit");
    assert!(bed.slots.degraded().is_none());
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Lost);
    let journal = std::fs::read_to_string(bed.dir.0.join("state/journal.jsonl")).unwrap();
    assert!(journal.lines().last().unwrap().contains("\"state\":\"lost\""), "{journal}");
}

#[test]
fn no_credential_reaches_the_journal_the_audit_or_a_report() {
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec { name: "fake", slots: 1, capability: confined(), command: sh("exit 0") }],
    );
    bed.until(20, |s| s.jobs().count() == 1 && Bed::all_final(s));
    // A positive control: the unit got the canned credential.
    let canary = "fake-client";
    let cred = podrunner_fake::canned_credential(&podrunner_schema::JobId::new("j-1").unwrap(), "r");
    assert!(cred.files.iter().any(|f| f.content.expose().contains(canary)));
    for f in ["state/journal.jsonl", "state/audit.jsonl"] {
        let text = std::fs::read_to_string(bed.dir.0.join(f)).unwrap();
        assert!(!text.is_empty() && !text.contains(canary), "{f}");
    }
    let reports = serde_json::to_string(&bed.slots.reports).unwrap();
    assert!(!reports.contains(canary));
}

fn until_ms(f: impl Fn() -> bool) -> bool {
    (0..500).any(|_| {
        if f() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        false
    })
}
