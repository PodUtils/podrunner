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
    let real = json!({"run_id": 2, "job_name": "t", "event": "push", "repository": "example-org/app",
        "head_is_fork": false, "actor": "a"});
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    port.asks.lock().unwrap().push((unit_of(&bed), serde_json::to_vec(&real).unwrap(), tx));
    bed.tick();
    assert_eq!(rx.try_recv().unwrap(), r#"{"verdict":"keep"}"#);
    assert_eq!(bed.state("j-1"), Some(State::Running));
}

/// The first mark wins, and a `started` event of the source after it still
/// checks the runner: another runner ends the unit (the design test 10 of
/// the source contract).
#[test]
fn a_started_of_another_runner_after_the_mark_ends_the_unit() {
    let port = Marking::default();
    let lines = vec![
        offer(0, "j-1", json!({})),
        r#"{"at_s":6,"started":{"job_id":"j-1","runner_name":"someone-else"}}"#.to_string(),
    ];
    let mut bed = bed(&port, lines, "sleep 300");
    bed.until(5, |s| s.jobs().any(|j| j.state == State::Started));
    port.marks.lock().unwrap().push(unit_of(&bed));
    bed.tick();
    assert_eq!(bed.state("j-1"), Some(State::Running));
    bed.until(40, Bed::all_final);
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::JobMismatch);
}
