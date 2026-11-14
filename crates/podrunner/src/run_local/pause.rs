//! A pause in a job (T-150). `--break JOB:STEP` puts a step before the
//! step STEP (its number from 1, or its `id`) of the job JOB; with
//! `--break-on-failure`, a step after each step pauses after the first one
//! that fails. The pause step asks the broker of its job
//! (`github.api_url`) each second until the user releases the job: podrunner
//! names the paused job on stderr and goes on when the user presses Enter.
//! The unit keeps running, with its egress hold, while it waits.

use std::io::{BufRead, IsTerminal};
use std::time::Duration;

use podrunner_local::Job;
use podrunner_local::drive::{End, Executor};
use podrunner_local::exec::Live;
use podrunner_local::hold;
use podrunner_schema::JobId;
use serde_json::{Value, json};

/// The breaks of a run.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Breaks {
    /// `JOB:STEP` of each `--break`.
    pub at: Vec<(String, String)>,
    pub on_failure: bool,
}

impl Breaks {
    pub(super) fn is_empty(&self) -> bool {
        self.at.is_empty() && !self.on_failure
    }
}

/// The value of `--break`: `JOB:STEP`.
pub(super) fn parse(v: &str) -> Result<(String, String), String> {
    match v.split_once(':') {
        Some((j, s)) if !j.is_empty() && !s.is_empty() => Ok((j.to_string(), s.to_string())),
        _ => Err(format!("--break {v:?} is not JOB:STEP (the number of the step from 1, or its id)")),
    }
}

/// The pause step of the cell `cell`, run when `condition` holds.
fn pause_step(cell: &str, n: usize, condition: &str, what: &str) -> Value {
    let url = format!("${{{{ github.api_url }}}}{}{cell}", hold::PATH.trim_start_matches('/'));
    let script = format!(
        "echo \"podrunner: a pause {what}; press Enter in podrunner to go on\"\n\
         until curl -fsS -o /dev/null \"{url}\" 2>/dev/null || wget -q -O /dev/null \"{url}\" 2>/dev/null; do sleep 1; done"
    );
    let script = podrunner_local::message::token(&script).unwrap_or(Value::String(String::new()));
    let name = format!("__podrunner_pause_{n}");
    json!({
        "type": "action", "reference": { "type": "script" },
        "id": format!("a0000000-0000-0000-0000-00000f{n:06x}"), "name": name, "contextName": name,
        "displayName": format!("podrunner: a pause {what}"), "condition": condition,
        "inputs": { "type": 2, "map": [{ "Key": "script", "Value": script }] },
    })
}

/// Put the pause steps of `b` into the steps of `message`, the job of the
/// cell `cell` (`JOB.N`).
pub(super) fn insert(b: &Breaks, cell: &str, message: &mut Job) {
    let job = cell.split('.').next().unwrap_or(cell);
    let Some(Value::Array(steps)) = message.overrides.get_mut("steps") else { return };
    let mut out = Vec::with_capacity(steps.len() * 2);
    let mut n = 0;
    for (i, step) in steps.drain(..).enumerate() {
        let id = step["contextName"].as_str().unwrap_or("").to_string();
        let here = b.at.iter().any(|(j, s)| j == job && (*s == (i + 1).to_string() || *s == id));
        if here {
            n += 1;
            out.push(pause_step(cell, n, "success()", &format!("before step {} of {cell}", i + 1)));
        }
        out.push(step);
        if b.on_failure {
            n += 1;
            out.push(pause_step(cell, n, "failure()", &format!("after a failed step of {cell}")));
        }
    }
    *steps = out;
}

/// An executor that puts the pause steps into each job.
pub(super) struct Paused {
    pub inner: Box<dyn Executor>,
    pub breaks: Breaks,
}

impl Executor for Paused {
    fn run(&mut self, job: &JobId, mut message: Job, limit: Duration) -> Result<End, String> {
        insert(&self.breaks, job.as_str(), &mut message);
        self.inner.run(job, message, limit)
    }
    fn select(&mut self, labels: &[String]) -> Result<(), String> {
        self.inner.select(labels)
    }
    fn concurrency(&mut self, group: Option<&podrunner_local::concurrency::Group>) {
        self.inner.concurrency(group)
    }
    fn environment(&mut self, name: Option<&str>) {
        self.inner.environment(name)
    }
    fn set_live(&mut self, live: Live) {
        self.inner.set_live(live)
    }
    fn set_stop(&mut self, stop: podrunner_local::cancel::Stop) {
        self.inner.set_stop(stop)
    }
    fn take_logs(&mut self) -> Vec<(JobId, String)> {
        self.inner.take_logs()
    }
    fn take_blobs(&mut self) -> Vec<(JobId, Vec<(String, String)>)> {
        self.inner.take_blobs()
    }
}

/// Name each paused job on stderr, and release it when the user presses
/// Enter; a signal releases each one. It runs until the process ends.
pub(super) fn watch() -> Result<(), String> {
    if !std::io::stdin().is_terminal() {
        return Err("--break and --break-on-failure need a terminal: podrunner waits for Enter".into());
    }
    std::thread::spawn(|| {
        loop {
            if podrunner_local::cancel::requested() {
                hold::release_all();
            }
            if let Some(job) = hold::waiting().into_iter().next() {
                eprintln!("podrunner: {job} is paused; its unit keeps running. Press Enter to go on.");
                let mut line = String::new();
                let _ = std::io::stdin().lock().read_line(&mut line);
                hold::release(&job);
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(m: &Job) -> Vec<(String, String)> {
        m.overrides["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                (s["contextName"].as_str().unwrap_or("").to_string(), s["condition"].as_str().unwrap_or("").to_string())
            })
            .collect()
    }

    fn message() -> Job {
        let mut m = Job::scripts("build", &[]);
        let step = |id: &str| json!({ "contextName": id, "condition": "success()" });
        m.overrides.insert("steps".into(), json!([step("a"), step("b"), step("c")]));
        m
    }

    #[test]
    fn the_pause_steps_go_before_a_step_and_after_each_failure() {
        let b = Breaks { at: vec![("build".into(), "2".into()), ("build".into(), "c".into())], on_failure: false };
        let mut m = message();
        insert(&b, "build.0", &mut m);
        let names: Vec<String> = steps(&m).into_iter().map(|s| s.0).collect();
        assert_eq!(names, ["a", "__podrunner_pause_1", "b", "__podrunner_pause_2", "c"]);
        let mut other = message();
        insert(&b, "test.0", &mut other);
        assert_eq!(steps(&other).len(), 3, "another job gets no pause");
        let f = Breaks { at: vec![], on_failure: true };
        let mut m = message();
        insert(&f, "build.0", &mut m);
        let s = steps(&m);
        assert_eq!(s.len(), 6);
        assert_eq!(s[1].1, "failure()");
        let script = m.overrides["steps"][1]["inputs"]["map"][0]["Value"].to_string();
        assert!(script.contains("github.api_url") && script.contains("_podrunner/hold/build.0"), "{script}");
        assert!(parse("build:2").is_ok() && parse("build").is_err() && parse(":2").is_err());
    }
}
