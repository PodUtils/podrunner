//! The hooks behind slots: a socket for each unit of a kind with
//! `container: hooks` and none for other kinds; the unit gets the hook
//! variables; a refusal is an audit line `hook-refuse`; `job-started` moves
//! `started` to `running`; destroy closes the socket.

use std::sync::{Arc, Mutex};

use podrunner_schema::{Boundary, ContainerRoute, EgressHold, Principal, RefusalCode, UnitId};
use serde_json::json;

use super::*;
use crate::audit;
use crate::hooks::{HookEvent, HookPort};
use crate::state::State;

/// What `take` gives, once each.
type Queued = Arc<Mutex<Vec<(Vec<RefusalCode>, Vec<HookEvent>)>>>;

#[derive(Clone, Default)]
struct Seen {
    opened: Arc<Mutex<Vec<String>>>,
    closed: Arc<Mutex<Vec<String>>>,
    queued: Queued,
    images: Arc<Mutex<Vec<String>>>,
}

impl HookPort for Seen {
    fn open(
        &self,
        kind: &str,
        _driver: Arc<dyn podrunner_unit::UnitDriver>,
        unit: &UnitId,
        socket: &std::path::Path,
        _guest: Option<&str>,
    ) -> Result<std::collections::BTreeMap<String, String>, String> {
        self.opened.lock().unwrap().push(format!("{kind} {unit}"));
        Ok([("PODRUNNER_CTL".to_string(), socket.display().to_string())].into())
    }
    fn close(&self, unit: &UnitId) {
        self.closed.lock().unwrap().push(unit.to_string());
    }
    fn take(&self, _unit: &UnitId) -> (Vec<RefusalCode>, Vec<HookEvent>) {
        self.queued.lock().unwrap().pop().unwrap_or_default()
    }
    fn images(&self, _unit: &UnitId) -> Vec<String> {
        std::mem::take(&mut self.images.lock().unwrap())
    }
}

fn bed(container: ContainerRoute, command: &str) -> (Bed, Seen) {
    let mut cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    cap.container = container;
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec { name: "fake", slots: 1, capability: cap, command: sh(command) }],
    );
    let seen = Seen::default();
    bed.slots.set_hooks(Box::new(seen.clone()));
    (bed, seen)
}

#[test]
fn a_unit_with_hooks_gets_a_socket_and_its_events() {
    // The unit sees the socket in its environment.
    let (mut b, seen) = bed(ContainerRoute::Hooks, "test -n \"$PODRUNNER_CTL\" && sleep 1");
    b.until(10, |s| s.jobs().any(|j| j.state == State::Started));
    assert_eq!(seen.opened.lock().unwrap().len(), 1);
    seen.queued.lock().unwrap().push((vec![RefusalCode::BuildNotSupported], vec![HookEvent::JobStarted]));
    b.tick();
    assert!(
        b.slots.jobs().all(|j| j.state == State::Running),
        "{:?}",
        b.slots.jobs().map(|j| j.state).collect::<Vec<_>>()
    );
    let refuse: Vec<_> = b.slots.audit_lines().iter().filter(|l| l.audit == audit::Kind::HookRefuse).collect();
    assert_eq!(refuse.len(), 1);
    assert_eq!(refuse[0].code, Some(RefusalCode::BuildNotSupported));
    b.until(400, Bed::all_final);
    // The socket closes at destroy, and the unit ran to its end with the
    // variable set.
    assert_eq!(seen.closed.lock().unwrap().len(), 1);
    assert_eq!(b.slots.reports[0].exit_reason, podrunner_schema::ExitReason::Success);
}

/// The design test 14 of the unit contract (T-053): `job-started` in
/// `running` is ignored, and `job-completed` goes to the audit with its
/// time and moves no state.
#[test]
fn job_started_in_running_is_ignored_and_job_completed_moves_no_state() {
    let (mut b, seen) = bed(ContainerRoute::Hooks, "sleep 2");
    b.until(10, |s| s.jobs().any(|j| j.state == State::Started));
    seen.queued.lock().unwrap().push((vec![], vec![HookEvent::JobStarted]));
    b.tick();
    assert!(b.slots.jobs().all(|j| j.state == State::Running));
    seen.queued.lock().unwrap().push((vec![], vec![HookEvent::JobStarted, HookEvent::JobCompleted]));
    b.tick();
    assert!(b.slots.jobs().all(|j| j.state == State::Running));
    let done: Vec<_> = b.slots.audit_lines().iter().filter(|l| l.audit == audit::Kind::JobCompleted).collect();
    assert_eq!(done.len(), 1);
    assert!(done[0].t_ms > 0);
    b.until(400, Bed::all_final);
}

#[test]
fn a_unit_with_no_container_route_gets_no_socket() {
    let (mut b, seen) = bed(ContainerRoute::None, "test -z \"$PODRUNNER_CTL\"");
    b.until(50, |s| s.jobs().count() == 1 && Bed::all_final(s));
    assert!(seen.opened.lock().unwrap().is_empty());
    assert_eq!(b.slots.reports[0].exit_reason, podrunner_schema::ExitReason::Success);
}

/// T-075: the report names the time from the claim to the first step (the
/// `job-started` hook) and the digest of each image of a container, once.
#[test]
fn report_fields_name_the_first_step_and_the_job_images() {
    let digest = format!("sha256:{}", "a".repeat(64));
    let (mut b, seen) = bed(ContainerRoute::Hooks, "sleep 2");
    b.until(10, |s| s.jobs().any(|j| j.state == State::Started));
    for _ in 0..3 {
        b.tick();
    }
    seen.images.lock().unwrap().extend([digest.clone(), digest.clone()]);
    seen.queued.lock().unwrap().push((vec![], vec![HookEvent::JobStarted]));
    b.tick();
    b.until(400, Bed::all_final);
    let r = &b.slots.reports[0];
    assert!(r.claim_to_first_step_s.is_some_and(|s| s >= 3), "{:?}", r.claim_to_first_step_s);
    assert_eq!(r.job_images, [digest]);
    assert!(r.check().is_ok(), "{:?}", r.check());
    // The public report leaves both out.
    let public = serde_json::to_value(r.public()).unwrap();
    assert!(public.get("job_images").is_none() && public.get("claim_to_first_step_s").is_none());
}
