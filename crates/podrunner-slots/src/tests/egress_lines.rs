//! T-486 (githome's G-04): the egress lines of a unit reach the source of
//! its job, at most 1000, then one line with the count of the rest.

use std::sync::{Arc, Mutex};

use podrunner_schema::{Boundary, EgressHold, Principal};
use serde_json::{Map, Value, json};

use super::*;
use crate::state::State;

type Seen = Arc<Mutex<Vec<(String, Map<String, Value>)>>>;

/// A source that keeps each audit line it gets.
struct Keeping(Box<dyn podrunner_source::JobSource>, String, Seen);

macro_rules! pass {
    ($($name:ident($($a:ident: $t:ty),*) -> $r:ty;)*) => {$(
        fn $name(&mut self, $($a: $t),*) -> $r { self.0.$name($($a),*) }
    )*};
}

impl podrunner_source::JobSource for Keeping {
    pass! {
        hello(c: &str, v: &str) -> Result<podrunner_source::Hello, podrunner_source::SourceError>;
        next(n: u32, w: std::time::Duration) -> Result<Vec<podrunner_source::Event>, podrunner_source::SourceError>;
        ack(j: &podrunner_schema::JobId) -> Result<(), podrunner_source::SourceError>;
        claim(j: &podrunner_schema::JobId, r: &str) -> Result<(), podrunner_source::SourceError>;
        mint(j: &podrunner_schema::JobId, r: &str) -> Result<podrunner_schema::Credential, podrunner_source::SourceError>;
        report(j: &podrunner_schema::JobId, r: &Map<String, Value>) -> Result<(), podrunner_source::SourceError>;
        withdraw(j: &podrunner_schema::JobId, r: &str) -> Result<(), podrunner_source::SourceError>;
        release(j: &podrunner_schema::JobId) -> Result<(), podrunner_source::SourceError>;
    }
    fn audit(&mut self, l: &Map<String, Value>) -> Result<(), podrunner_source::SourceError> {
        if l.contains_key("verdict") || l.contains_key("dropped") {
            self.2.lock().unwrap().push((self.1.clone(), l.clone()));
        }
        self.0.audit(l)
    }
}

/// An egress module that gives `n` allowed lines and one denied line for
/// the first unit it opens, once.
#[derive(Clone, Default)]
struct Giving {
    unit: Arc<Mutex<Option<String>>>,
    n: usize,
}

impl crate::EgressPort for Giving {
    fn open(&self, unit: &str, _: &str, _: &std::path::Path) -> Result<usize, String> {
        *self.unit.lock().unwrap() = Some(unit.into());
        Ok(0)
    }
    fn close(&self, _: &str) {}
    fn alive(&self) -> bool {
        true
    }
    fn lines(&self) -> Vec<(String, Map<String, Value>)> {
        let Some(u) = self.unit.lock().unwrap().take() else { return Vec::new() };
        let line = |verdict: &str| {
            let v = json!({ "host": "github.com", "port": 443, "verdict": verdict, "rule": "r",
                "bytes_in": 1, "bytes_out": 2, "at": "2026-11-18T00:00:00Z" });
            (u.clone(), v.as_object().unwrap().clone())
        };
        let mut out: Vec<_> = (0..self.n).map(|_| line("allow")).collect();
        out.push(line("deny"));
        out
    }
}

fn run(n: usize) -> Vec<(String, Map<String, Value>)> {
    let open = "[egress]\ndefault_policy = { schema = 1, mode = \"open\" }";
    let bed = Bed::new(
        config(open),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))]), ("b", "trusted", None, vec![])],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory),
            command: sh("exit 0"),
        }],
    );
    let seen: Seen = Default::default();
    let mut bed = Bed { slots: bed.slots.with_egress(Box::new(Giving { n, ..Default::default() })), ..bed };
    for (i, name) in ["a", "b"].iter().enumerate() {
        let inner = std::mem::replace(
            &mut bed.slots.sources[i].source,
            Box::new(podrunner_fake::FakeSource::new("x", bed.clock.clone(), vec![])),
        );
        bed.slots.sources[i].source = Box::new(Keeping(inner, name.to_string(), seen.clone()));
    }
    bed.until(30, |s| s.jobs().count() == 1 && s.jobs().all(|j| j.state == State::Reported));
    bed.tick();
    seen.lock().unwrap().clone()
}

#[test]
fn the_egress_lines_of_a_unit_reach_the_source_of_its_job() {
    let got = run(1);
    assert_eq!(got.len(), 2, "{got:?}");
    assert!(got.iter().all(|(s, l)| s == "a" && l["job_id"] == "j-1"), "{got:?}");
    let verdicts: Vec<&str> = got.iter().map(|(_, l)| l["verdict"].as_str().unwrap()).collect();
    assert_eq!(verdicts, ["allow", "deny"]);
}

#[test]
fn a_job_gets_at_most_1000_lines_and_the_count_of_the_rest() {
    let got = run(1001);
    let lines = got.iter().filter(|(_, l)| l.contains_key("verdict")).count();
    assert_eq!(lines, crate::egress_lines::PER_JOB as usize);
    let dropped: Vec<_> = got.iter().filter(|(_, l)| l.contains_key("dropped")).collect();
    assert_eq!(dropped.len(), 1, "{dropped:?}");
    assert_eq!(dropped[0].1["dropped"], 2, "1002 lines, 1000 sent");
}
