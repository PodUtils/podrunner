//! The denials of the egress module: each connection that a policy refused,
//! with its job, its host, its port and its rule. A job log names them, so
//! a user sees which host to allow. The audit of `pod-broker` goes on to
//! `tracing` as before; only the last denials stay in memory. A connection
//! that failed (a stage and a class) is a line of the log of `serve`.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use pod_broker::{Audit, Line, Tracing};

/// How many denials stay.
const KEEP: usize = 1000;

/// The audit sink of the module.
#[derive(Default)]
pub struct Denials {
    last: Mutex<VecDeque<(String, String)>>,
    /// The line of each connection, for the source of its job (T-486).
    pub(crate) lines: crate::lines::Lines,
    /// No line on stderr for each failed connection: the local mode names
    /// the refused hosts of a failed job at its end (T-328, T-329).
    quiet: AtomicBool,
}

impl Denials {
    /// Write no line for each failed connection.
    pub fn quiet(&self) {
        self.quiet.store(true, Ordering::Relaxed);
    }

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

/// The text for the log of `serve` of a connection that failed: its job,
/// its host and port, and the stage and the class of the failure (T-179).
/// An audit line of `pod-broker` holds no secret value.
pub fn failure_text(line: &Line) -> Option<String> {
    let stage = line.stage?;
    let f = &line.fields;
    let text = |k: &str| f[k].as_str().map(str::to_string).unwrap_or_else(|| f[k].to_string());
    Some(format!(
        "podrunner: egress: job {} {}:{} failed at {stage} ({})",
        text("job"),
        text("host"),
        text("port"),
        line.class.unwrap_or("unknown")
    ))
}

impl Audit for Denials {
    fn line(&self, line: &Line) {
        Tracing.line(line);
        self.lines.keep(line, &pod_core::clock::rfc3339(std::time::SystemTime::now()));
        if let Some(t) = failure_text(line).filter(|_| !self.quiet.load(Ordering::Relaxed)) {
            eprintln!("{t}");
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_connection_is_a_line_of_the_log() {
        let line = |stage: Option<&'static str>| Line {
            schema: 1,
            time: "2026-11-15T00:00:00Z".into(),
            level: "warn",
            event: "broker.connection",
            fields: serde_json::json!({ "job": "u-1", "host": "github.com", "port": 443, "decision": "allow" }),
            stage,
            class: Some("unavailable"),
        };
        assert_eq!(
            failure_text(&line(Some("connect"))).as_deref(),
            Some("podrunner: egress: job u-1 github.com:443 failed at connect (unavailable)")
        );
        assert_eq!(failure_text(&line(None)), None);
    }
}
