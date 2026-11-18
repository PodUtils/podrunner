//! T-493 (githome's G-15): the answer to the job-started hook of a unit
//! whose runner got a job of another run.

use podrunner_schema::{Boundary, EgressHold, ExitReason, Principal, UnitId};
use podrunner_source::RealJob;
use serde_json::json;

use super::*;
use crate::assigned::Verdict;
use crate::audit::Kind as AuditKind;
use crate::state::State;

fn running() -> (Bed, UnitId) {
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory),
            command: sh("exec sleep 300"),
        }],
    );
    bed.until(10, |s| s.jobs().any(|j| j.state == State::Started));
    let unit = bed.slots.jobs().next().and_then(|j| j.unit.clone()).expect("a unit");
    (bed, unit)
}

fn real(repository: &str, run_id: u64, fork: bool) -> RealJob {
    RealJob {
        run_id,
        job_name: "test".into(),
        event: if fork { "pull_request" } else { "push" }.into(),
        repository: repository.into(),
        head_repository: None,
        head_is_fork: Some(fork),
        actor: "a".into(),
        job_id: None,
    }
}

fn end(code: &str) -> Verdict {
    Verdict::End { code: code.into() }
}

#[test]
fn the_run_of_the_offer_and_a_trusted_run_are_kept() {
    let (mut bed, unit) = running();
    assert_eq!(bed.slots.assigned(&unit, &real("Example-Org/app", 1, false)), Verdict::Keep);
    let (mut bed2, unit2) = running();
    assert_eq!(bed2.slots.assigned(&unit2, &real("example-org/app", 2, false)), Verdict::Keep);
    let line = bed2.slots.audit_lines().iter().find(|a| a.audit == AuditKind::Assigned).expect("an audit line");
    assert_eq!(line.detail.as_deref(), Some("run 2 of example-org/app: keep"));
    drop(bed.slots);
}

#[test]
fn a_fork_in_a_trusted_unit_ends_and_the_answer_is_kept() {
    let (mut bed, unit) = running();
    assert_eq!(bed.slots.assigned(&unit, &real("example-org/app", 3, true)), end("tier-mismatch"));
    // A second ask (a step of the job) gets the same answer.
    assert_eq!(bed.slots.assigned(&unit, &real("example-org/app", 1, false)), end("tier-mismatch"));
    let j = bed.slots.jobs().next().unwrap();
    assert_eq!(j.reason, Some(ExitReason::JobMismatch));
}

#[test]
fn the_source_ends_a_job_of_another_owner() {
    let (mut bed, unit) = running();
    assert_eq!(bed.slots.assigned(&unit, &real("someone-else/app", 4, false)), end("not-of-this-source"));
}

#[test]
fn an_unknown_unit_and_a_bad_fact_end() {
    let (mut bed, unit) = running();
    let other = UnitId::parse("u-999").unwrap();
    assert_eq!(bed.slots.assigned(&other, &real("example-org/app", 1, false)), end("unknown-unit"));
    assert_eq!(bed.slots.assigned(&unit, &real("no-slash", 5, false)), end("bad-request"));
    let mut long = real("example-org/app", 5, false);
    long.job_name = "x".repeat(300);
    let (mut bed2, unit2) = running();
    assert_eq!(bed2.slots.assigned(&unit2, &long), end("bad-request"));
}

/// An egress module that holds one question for the first unit it opens.
#[derive(Clone, Default)]
struct Asking {
    queued: std::sync::Arc<std::sync::Mutex<Vec<crate::assigned::Question>>>,
    body: Vec<u8>,
    answer: std::sync::Arc<std::sync::Mutex<Option<std::sync::mpsc::Receiver<String>>>>,
}

impl crate::EgressPort for Asking {
    fn open(&self, unit: &str, _: &str, _: &std::path::Path) -> Result<usize, String> {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.queued.lock().unwrap().push((UnitId::parse(unit).unwrap(), self.body.clone(), tx));
        *self.answer.lock().unwrap() = Some(rx);
        Ok(0)
    }
    fn close(&self, _: &str) {}
    fn alive(&self) -> bool {
        true
    }
    fn questions(&self) -> Vec<crate::assigned::Question> {
        std::mem::take(&mut *self.queued.lock().unwrap())
    }
}

/// The question of a unit comes through its egress socket and the tick
/// answers it; a body that is not a real job ends the job.
#[test]
fn a_question_of_the_egress_socket_is_answered_at_the_tick() {
    let open = "[egress]\ndefault_policy = { schema = 1, mode = \"open\" }";
    for (body, want) in [
        (serde_json::to_vec(&real("example-org/app", 9, true)).unwrap(), r#"{"code":"tier-mismatch","verdict":"end"}"#),
        (b"not json".to_vec(), r#"{"code":"bad-request","verdict":"end"}"#),
    ] {
        let port = Asking { body, ..Default::default() };
        let bed = Bed::new(
            config(open),
            vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
            vec![KindSpec {
                name: "fake",
                slots: 1,
                capability: capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory),
                command: sh("exec sleep 300"),
            }],
        );
        let mut bed = Bed { slots: bed.slots.with_egress(Box::new(port.clone())), ..bed };
        bed.until(10, |s| s.jobs().any(|j| j.state == State::Started));
        bed.tick();
        let rx = port.answer.lock().unwrap().take().expect("a question");
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), want);
    }
}

/// A source that answers `keep` with the facts of the offer again.
struct Stale(Box<dyn podrunner_source::JobSource>, podrunner_schema::offer::TrustInputs);

impl podrunner_source::JobSource for Stale {
    fn hello(&mut self, c: &str, v: &str) -> Result<podrunner_source::Hello, podrunner_source::SourceError> {
        self.0.hello(c, v)
    }
    fn next(
        &mut self,
        n: u32,
        w: std::time::Duration,
    ) -> Result<Vec<podrunner_source::Event>, podrunner_source::SourceError> {
        self.0.next(n, w)
    }
    fn ack(&mut self, j: &podrunner_schema::JobId) -> Result<(), podrunner_source::SourceError> {
        self.0.ack(j)
    }
    fn claim(&mut self, j: &podrunner_schema::JobId, r: &str) -> Result<(), podrunner_source::SourceError> {
        self.0.claim(j, r)
    }
    fn mint(
        &mut self,
        j: &podrunner_schema::JobId,
        r: &str,
    ) -> Result<podrunner_schema::Credential, podrunner_source::SourceError> {
        self.0.mint(j, r)
    }
    fn report(
        &mut self,
        j: &podrunner_schema::JobId,
        r: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), podrunner_source::SourceError> {
        self.0.report(j, r)
    }
    fn audit(&mut self, l: &serde_json::Map<String, serde_json::Value>) -> Result<(), podrunner_source::SourceError> {
        self.0.audit(l)
    }
    fn withdraw(&mut self, j: &podrunner_schema::JobId, r: &str) -> Result<(), podrunner_source::SourceError> {
        self.0.withdraw(j, r)
    }
    fn release(&mut self, j: &podrunner_schema::JobId) -> Result<(), podrunner_source::SourceError> {
        self.0.release(j)
    }
    fn assigned(
        &mut self,
        _: &podrunner_schema::JobId,
        _: &RealJob,
    ) -> Result<podrunner_source::Assigned, podrunner_source::SourceError> {
        Ok(podrunner_source::Assigned::Keep(self.1.clone()))
    }
}

/// The review of T-493: the facts of `keep` must be of the real job.
#[test]
fn a_keep_with_the_facts_of_the_offer_ends_the_job() {
    let (mut bed, unit) = running();
    let facts = bed.slots.jobs().next().and_then(|j| j.offer.clone()).unwrap().trust_inputs;
    let inner = std::mem::replace(
        &mut bed.slots.sources[0].source,
        Box::new(podrunner_fake::FakeSource::new(
            "x",
            std::sync::Arc::new(pod_core::clock::ManualClock::new()),
            vec![],
        )),
    );
    bed.slots.sources[0].source = Box::new(Stale(inner, facts));
    assert_eq!(bed.slots.assigned(&unit, &real("example-org/app", 7, true)), end("facts-mismatch"));
}

/// The review of T-493: when the hook gave up before the answer, the job
/// ends as the hook said, not with the answer that came late.
#[test]
fn a_late_answer_records_the_end_of_the_hook() {
    let open = "[egress]\ndefault_policy = { schema = 1, mode = \"open\" }";
    let port = Asking { body: serde_json::to_vec(&real("example-org/app", 2, false)).unwrap(), ..Default::default() };
    let bed = Bed::new(
        config(open),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory),
            command: sh("exec sleep 300"),
        }],
    );
    let mut bed = Bed { slots: bed.slots.with_egress(Box::new(port.clone())), ..bed };
    bed.until(10, |s| s.jobs().any(|j| j.state == State::Started));
    // The hook gave up: its end of the channel is gone.
    drop(port.answer.lock().unwrap().take());
    bed.tick();
    let j = bed.slots.jobs().next().unwrap();
    assert_eq!(j.assigned, Some(end("source-unavailable")));
    assert_eq!(j.reason, Some(ExitReason::JobMismatch));
}
