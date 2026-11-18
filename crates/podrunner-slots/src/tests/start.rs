//! T-176: a unit that does not start after the mint: the source gets
//! `release` of the job (the sidecar removes the runner), and the audit
//! names the cause of the driver.

use podrunner_schema::{Boundary, EgressHold, Principal, RefusalCode};
use serde_json::json;

use super::*;

#[test]
fn a_failed_start_releases_the_job_and_names_the_cause() {
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: cap,
            command: vec!["/nonexistent/podrunner-payload".into()],
        }],
    );
    bed.until(50, |s| s.jobs().all(|j| j.state.is_final()) && s.jobs().count() == 1);
    let r = bed.slots.reports.iter().find(|r| r.job_id.as_str() == "j-1").expect("a report");
    assert_eq!(r.refusal_code, Some(RefusalCode::UnitStartFailed), "{r:?}");
    let audit = std::fs::read_to_string(bed.slots.state_dir.join("audit.jsonl")).unwrap();
    assert!(audit.contains("start: "), "the audit names no cause: {audit}");
}

/// T-197: a unit whose runner does not take its job in `start_s` ends
/// `unit-failed`, and the audit names the deadline.
#[test]
fn the_start_deadline_names_itself() {
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let mut bed = Bed::new(
        config("[deadlines]\nstart_s = 3\n"),
        vec![("a", "trusted", None, vec![offer(0, "j-2", json!({}))])],
        vec![KindSpec { name: "fake", slots: 1, capability: cap, command: sh("sleep 30") }],
    );
    bed.until(30, |s| s.jobs().all(|j| j.state.is_final()) && s.jobs().count() == 1);
    let r = bed.slots.reports.iter().find(|r| r.job_id.as_str() == "j-2").expect("a report");
    assert_eq!(r.exit_reason, podrunner_schema::ExitReason::UnitFailed, "{r:?}");
    let audit = std::fs::read_to_string(bed.slots.state_dir.join("audit.jsonl")).unwrap();
    assert!(audit.contains("[deadlines] start_s (3 s)"), "{audit}");
}

/// A source that logs the order of `report` and `release`.
struct Order {
    inner: Box<dyn podrunner_source::JobSource>,
    calls: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
}

impl podrunner_source::JobSource for Order {
    fn hello(&mut self, c: &str, v: &str) -> Result<podrunner_source::Hello, podrunner_source::SourceError> {
        self.inner.hello(c, v)
    }
    fn next(
        &mut self,
        n: u32,
        w: std::time::Duration,
    ) -> Result<Vec<podrunner_source::Event>, podrunner_source::SourceError> {
        self.inner.next(n, w)
    }
    fn ack(&mut self, j: &podrunner_schema::JobId) -> Result<(), podrunner_source::SourceError> {
        self.inner.ack(j)
    }
    fn claim(&mut self, j: &podrunner_schema::JobId, r: &str) -> Result<(), podrunner_source::SourceError> {
        self.inner.claim(j, r)
    }
    fn mint(
        &mut self,
        j: &podrunner_schema::JobId,
        r: &str,
    ) -> Result<podrunner_schema::Credential, podrunner_source::SourceError> {
        self.inner.mint(j, r)
    }
    fn report(
        &mut self,
        j: &podrunner_schema::JobId,
        r: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), podrunner_source::SourceError> {
        self.calls.lock().unwrap().push("report");
        self.inner.report(j, r)
    }
    fn audit(&mut self, l: &serde_json::Map<String, serde_json::Value>) -> Result<(), podrunner_source::SourceError> {
        self.inner.audit(l)
    }
    fn withdraw(&mut self, j: &podrunner_schema::JobId, r: &str) -> Result<(), podrunner_source::SourceError> {
        self.inner.withdraw(j, r)
    }
    fn release(&mut self, j: &podrunner_schema::JobId) -> Result<(), podrunner_source::SourceError> {
        self.calls.lock().unwrap().push("release");
        self.inner.release(j)
    }
}

/// T-200: a claimed job that cannot start gets its report (with the
/// refusal) before `release`, as a job that ran: at the `release`, the
/// source knows whether podrunner refused the job.
#[test]
fn a_refused_claim_is_reported_before_its_release() {
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-3", json!({}))])],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: cap,
            command: vec!["/nonexistent/podrunner-payload".into()],
        }],
    );
    let calls = std::sync::Arc::default();
    let empty = podrunner_fake::parse_script("").unwrap();
    let placeholder = podrunner_fake::FakeSource::new("a", bed.clock.clone(), empty);
    let inner = std::mem::replace(&mut bed.slots.sources[0].source, Box::new(placeholder));
    bed.slots.sources[0].source = Box::new(Order { inner, calls: std::sync::Arc::clone(&calls) });
    bed.until(50, |s| s.jobs().all(|j| j.state.is_final()) && s.jobs().count() == 1);
    assert_eq!(*calls.lock().unwrap(), ["report", "release"]);
}
