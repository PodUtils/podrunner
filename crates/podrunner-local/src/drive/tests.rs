use std::collections::BTreeMap;

use pod_actions_workflow::{Base, JobResult};
use podrunner_broker_fake::Job;
use podrunner_schema::JobId;

use super::*;
use crate::LocalSource;

/// An executor of the test: each job gets the result of its name, and the
/// scripts of its steps are kept.
#[derive(Default)]
struct Scripted {
    results: BTreeMap<String, JobResult>,
    ran: Vec<String>,
}

impl Executor for Scripted {
    fn run(&mut self, job: &JobId, message: Job) -> Result<End, String> {
        self.ran.push(job.to_string());
        let steps = message.overrides["steps"].as_array().unwrap().len();
        assert!(steps > 0);
        let id = job.to_string();
        let result = *self.results.get(&id).unwrap_or(&JobResult::Success);
        if id == "boom.0" {
            return Err("the unit did not start".into());
        }
        Ok(End { result, outputs: vec![] })
    }
}

const FLOW: &str = "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: echo a}]\n  \
                    b:\n    needs: a\n    runs-on: x\n    steps: [{uses: actions/checkout@v4}]\n  \
                    c:\n    needs: b\n    runs-on: x\n    steps: [{run: echo c}]\n  \
                    d:\n    needs: b\n    if: failure()\n    runs-on: x\n    steps: [{run: echo d}]\n";

#[test]
fn drive_runs_each_job_in_order_and_gives_each_line() {
    let mut s = LocalSource::new("l", FLOW, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    let mut exec = Scripted::default();
    let mut lines = Vec::new();
    let ok = drive(&mut s, &mut exec, &mut |l| lines.push(l.clone())).unwrap();
    assert!(!ok, "b failed");
    // `b` has an action: it fails before it runs, with its reason; `c`
    // is skipped; `d` (failure()) runs.
    assert_eq!(exec.ran, ["a.0", "d.0"]);
    assert!(matches!(&lines[0], Line::Ran { result: JobResult::Success, .. }), "{lines:?}");
    assert!(matches!(&lines[1], Line::Refused { reason, .. } if reason.contains("actions/checkout@v4")), "{lines:?}");
    assert!(lines.contains(&Line::Skipped { job: "c".into(), condition: "success()".into() }), "{lines:?}");
    assert!(lines.iter().any(|l| matches!(l, Line::Ran { job, .. } if job.to_string() == "d.0")), "{lines:?}");
    assert!(s.is_finished());
}

#[test]
fn an_executor_error_fails_the_job_and_a_success_is_true() {
    let text = "on: push\njobs:\n  boom:\n    runs-on: x\n    steps: [{run: x}]\n";
    let mut s = LocalSource::new("l", text, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    let mut lines = Vec::new();
    assert!(!drive(&mut s, &mut Scripted::default(), &mut |l| lines.push(l.clone())).unwrap());
    assert!(matches!(&lines[0], Line::Refused { reason, .. } if reason == "the unit did not start"));
    let ok = "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
    let mut s = LocalSource::new("l", ok, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    assert!(drive(&mut s, &mut Scripted::default(), &mut |_| {}).unwrap());
}

#[test]
fn the_conclusion_of_the_runner_gives_the_result() {
    assert_eq!(result_of("succeeded"), JobResult::Success);
    assert_eq!(result_of("failed"), JobResult::Failure);
    assert_eq!(result_of("cancelled"), JobResult::Cancelled);
    assert_eq!(result_of(""), JobResult::Failure);
}
