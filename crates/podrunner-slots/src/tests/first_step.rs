//! T-660 (githome's G-11 and G-25): the job-started hook of the runner
//! image marks the first step of its own job at the egress socket of its
//! unit. A kind with no container hook (incus, microvm, fake) and a source
//! with no `started` event still get the line `running` and
//! `claim_to_first_step_s`.

use std::sync::{Arc, Mutex};

use podrunner_schema::{Boundary, EgressHold, ExitReason, Principal, UnitId};
use serde_json::json;

use super::*;
use crate::state::State;

/// An egress module that gives the marks of `marks` and the questions of
/// `asks` at the next call.
#[derive(Clone, Default)]
struct Marking {
    marks: Arc<Mutex<Vec<UnitId>>>,
    asks: Arc<Mutex<Vec<crate::assigned::Question>>>,
}

impl crate::EgressPort for Marking {
    fn open(&self, _: &str, _: &str, _: &std::path::Path) -> Result<usize, String> {
        Ok(0)
    }
    fn close(&self, _: &str) {}
    fn alive(&self) -> bool {
        true
    }
    fn questions(&self) -> Vec<crate::assigned::Question> {
        std::mem::take(&mut *self.asks.lock().unwrap())
    }
    fn first_steps(&self) -> Vec<UnitId> {
        std::mem::take(&mut *self.marks.lock().unwrap())
    }
}

/// One fake job whose unit runs `command`, with no `started` event and no
/// container hook, behind the egress module `port`.
fn bed(port: &Marking, lines: Vec<String>, command: &str) -> Bed {
    let open = "[egress]\ndefault_policy = { schema = 1, mode = \"open\" }";
    let bed = Bed::new(
        config(open),
        vec![("a", "trusted", None, lines)],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory),
            command: sh(command),
        }],
    );
    Bed { slots: bed.slots.with_egress(Box::new(port.clone())), ..bed }
}

fn journal(bed: &Bed) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(bed.dir.0.join("state/journal.jsonl")).unwrap();
    text.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn unit_of(bed: &Bed) -> UnitId {
    bed.slots.jobs().next().and_then(|j| j.unit.clone()).expect("a unit")
}

#[test]
fn the_mark_of_the_hook_gives_the_running_line_and_the_first_step() {
    let port = Marking::default();
    let mut bed = bed(&port, vec![offer(0, "j-1", json!({}))], "sleep 2");
    bed.until(10, |s| s.jobs().any(|j| j.state == State::Started));
    bed.attest_assignment_hook();
    // A mark of a unit with no job in `started` (a warm unit before its
    // claim) changes nothing.
    port.marks.lock().unwrap().push(UnitId::parse("u-999").unwrap());
    for _ in 0..2 {
        bed.tick();
    }
    assert_eq!(bed.state("j-1"), Some(State::Started));
    let unit = unit_of(&bed);
    // Two marks (a step of the job that calls again): one line.
    port.marks.lock().unwrap().extend([unit.clone(), unit.clone()]);
    bed.tick();
    assert_eq!(bed.state("j-1"), Some(State::Running));
    port.marks.lock().unwrap().push(unit);
    bed.tick();
    bed.until(400, Bed::all_final);
    let lines = journal(&bed);
    let running = lines.iter().filter(|l| l["job_id"] == "j-1" && l["state"] == "running").count();
    assert_eq!(running, 1, "{lines:?}");
    let at = |s: &str| lines.iter().position(|l| l["job_id"] == "j-1" && l["state"] == s).unwrap();
    assert!(at("claimed") < at("running") && at("running") < at("ended"), "{lines:?}");
    let r = &bed.slots.reports[0];
    assert_eq!(r.exit_reason, ExitReason::Success);
    assert!(r.claim_to_first_step_s.is_some_and(|s| s >= 3), "{:?}", r.claim_to_first_step_s);
    assert!(!r.unknown_reasons.contains_key("claim_to_first_step_s"), "{:?}", r.unknown_reasons);
}

/// The hook asks for a job of another run, and sends no mark after `keep`:
/// the answer `keep` is the first step.
#[test]
fn a_kept_question_is_the_first_step() {
    let port = Marking::default();
    let mut bed = bed(&port, vec![offer(0, "j-1", json!({}))], "sleep 300");
    bed.until(10, |s| s.jobs().any(|j| j.state == State::Started));
    bed.attest_assignment_hook();
    let real = json!({"run_id": 2, "job_name": "t", "event": "push", "repository": "example-org/app",
        "head_is_fork": false, "actor": "a"});
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    port.asks.lock().unwrap().push((unit_of(&bed), serde_json::to_vec(&real).unwrap(), tx));
    bed.tick();
    assert_eq!(rx.try_recv().unwrap(), r#"{"verdict":"keep"}"#);
    assert_eq!(bed.state("j-1"), Some(State::Running));
}

/// The offer `j-1`, then at `at_s` a `started` of `j-1` that names another
/// runner.
fn placed_elsewhere(at_s: u64) -> Vec<String> {
    vec![
        offer(0, "j-1", json!({})),
        format!(r#"{{"at_s":{at_s},"started":{{"job_id":"j-1","runner_name":"someone-else"}}}}"#),
    ]
}

/// The question of the hook of the unit for the run `run_id` of the
/// repository of the offer, answered at one tick.
fn ask(bed: &mut Bed, port: &Marking, run_id: u64) -> String {
    let real = json!({"run_id": run_id, "job_name": "t", "event": "push", "repository": "example-org/app",
        "head_is_fork": false, "actor": "a"});
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    port.asks.lock().unwrap().push((unit_of(bed), serde_json::to_vec(&real).unwrap(), tx));
    bed.tick();
    rx.try_recv().unwrap()
}

fn swap_line(bed: &Bed) -> bool {
    bed.slots.audit_lines().iter().any(|a| {
        a.audit == crate::audit::Kind::Assigned && a.detail.as_deref().is_some_and(|d| d.contains("another runner"))
    })
}

/// Fail closed: a step of the job can send the mark, so a mark is no proof
/// that the runner of the unit took its own job. A `started` of the placed
/// job that names another runner after the mark ends the unit (the design
/// test 10 of the source contract); a swap in the placed run ends too.
#[test]
fn a_started_of_another_runner_after_the_mark_ends_the_unit() {
    let port = Marking::default();
    let mut bed = bed(&port, placed_elsewhere(6), "sleep 300");
    bed.until(5, |s| s.jobs().any(|j| j.state == State::Started));
    bed.attest_assignment_hook();
    port.marks.lock().unwrap().push(unit_of(&bed));
    bed.tick();
    assert_eq!(bed.state("j-1"), Some(State::Running));
    bed.until(40, Bed::all_final);
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::JobMismatch);
    assert!(!swap_line(&bed));
}

/// A question that names the run of the offer gets `keep` with no call of
/// the source: it is no proof either, and the unit ends.
#[test]
fn a_keep_of_the_placed_run_then_a_started_of_another_runner_ends_the_unit() {
    let port = Marking::default();
    let mut bed = bed(&port, placed_elsewhere(8), "sleep 300");
    bed.until(5, |s| s.jobs().any(|j| j.state == State::Started));
    bed.attest_assignment_hook();
    assert_eq!(ask(&mut bed, &port, 1), r#"{"verdict":"keep"}"#);
    assert_eq!(bed.state("j-1"), Some(State::Running));
    bed.until(40, Bed::all_final);
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::JobMismatch);
    assert!(!swap_line(&bed));
}

/// T-493: `keep` of a job of another run, whose facts the source vouched
/// for, then the source says that the placed job started on another
/// runner: the kept job runs on, with an audit line.
#[test]
fn a_kept_job_runs_on_when_the_placed_job_starts_elsewhere() {
    let port = Marking::default();
    let mut bed = bed(&port, placed_elsewhere(8), "sleep 2");
    bed.until(5, |s| s.jobs().any(|j| j.state == State::Started));
    bed.attest_assignment_hook();
    assert_eq!(ask(&mut bed, &port, 2), r#"{"verdict":"keep"}"#);
    bed.until(400, Bed::all_final);
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success);
    assert!(swap_line(&bed), "an audit line names the placed job on another runner");
}

/// The design test 10 of the source contract holds after a mark: a
/// `started` that names the runner of this unit with another job id ends
/// the unit.
#[test]
fn the_runner_of_the_unit_on_another_job_still_ends_the_unit_after_the_mark() {
    let port = Marking::default();
    let lines = vec![
        offer(0, "j-1", json!({})),
        r#"{"at_s":6,"started":{"job_id":"j-9","runner_name":"podrunner-host-a-u-0"}}"#.to_string(),
    ];
    let mut bed = bed(&port, lines, "sleep 300");
    bed.until(5, |s| s.jobs().any(|j| j.state == State::Started));
    bed.attest_assignment_hook();
    port.marks.lock().unwrap().push(unit_of(&bed));
    bed.tick();
    assert_eq!(bed.state("j-1"), Some(State::Running));
    bed.until(40, Bed::all_final);
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::JobMismatch);
}

#[test]
fn an_unverified_payload_cannot_submit_a_hook_update() {
    let port = Marking::default();
    let mut bed = bed(&port, vec![offer(0, "j-1", json!({}))], "sleep 300");
    bed.until(5, |s| s.jobs().any(|j| j.state == State::Started));
    port.marks.lock().unwrap().push(unit_of(&bed));
    assert_eq!(ask(&mut bed, &port, 1), crate::assigned::Verdict::End { code: "no-hook".into() }.body());
    let job = bed.slots.jobs().next().unwrap();
    assert_eq!(job.state, State::Started);
    assert_eq!(job.reason, None);
    assert_eq!(job.assigned, None);
}
