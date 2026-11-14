//! The denials of the egress module: each connection that a policy refused,
//! with its job, its host, its port and its rule. A job log names them, so
//! a user sees which host to allow. The audit of `pod-broker` goes on to
//! `tracing` as before; only the last denials stay in memory.

use std::collections::VecDeque;
use std::sync::Mutex;

use pod_broker::{Audit, Line, Tracing};

/// How many denials stay.
const KEEP: usize = 1000;

/// The audit sink of the module.
#[derive(Default)]
pub struct Denials {
    last: Mutex<VecDeque<(String, String)>>,
}

impl Denials {
    /// `HOST:PORT (RULE)` of each denial of `job`, once each, in order.
    pub fn of(&self, job: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (j, d) in self.last.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            if j == job && !out.contains(d) {
                out.push(d.clone());
            }
        }
        out
    }
}

impl Audit for Denials {
    fn line(&self, line: &Line) {
        Tracing.line(line);
        let f = &line.fields;
        if f["decision"] != "deny" {
            return;
        }
        let text = |k: &str| match &f[k] {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => String::new(),
            v => v.to_string(),
        };
        let what = format!("{}:{} ({})", text("host"), text("port"), text("rule"));
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        if last.len() == KEEP {
            last.pop_front();
        }
        last.push_back((text("job"), what));
    }
}
