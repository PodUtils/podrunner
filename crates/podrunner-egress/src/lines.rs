//! The egress lines of each unit for the source of its job (T-486,
//! githome's G-04): one line for each connection when it ends, allowed,
//! denied or failed, with its host, port, rule and bytes. No path, header
//! or body of a request. The queue has a bound: a caller that does not
//! take the lines (the local mode) keeps the newest only.

use std::collections::VecDeque;
use std::sync::Mutex;

use pod_broker::Line;
use serde_json::{Map, Value, json};

/// The most lines that wait to be taken.
const WAITING: usize = 10_000;

/// The lines of the connections, by job, oldest first.
#[derive(Default)]
pub struct Lines(Mutex<VecDeque<(String, Map<String, Value>)>>);

impl Lines {
    /// Keep the line of a connection that ended (`broker.connection`
    /// with a decision), at the time `at` (RFC 3339 UTC).
    pub fn keep(&self, line: &Line, at: &str) {
        if line.event != "broker.connection" {
            return;
        }
        let f = &line.fields;
        let verdict = match (f["decision"].as_str(), line.stage) {
            (Some("deny"), _) => "deny",
            (Some("allow"), Some(_)) => "fail",
            (Some("allow"), None) => "allow",
            _ => return,
        };
        let Some(job) = f["job"].as_str() else { return };
        let mut out = Map::new();
        out.insert("host".into(), f["host"].clone());
        out.insert("port".into(), f["port"].clone());
        out.insert("verdict".into(), json!(verdict));
        out.insert("rule".into(), f["rule"].clone());
        out.insert("bytes_in".into(), json!(f["down"].as_u64().unwrap_or(0)));
        out.insert("bytes_out".into(), json!(f["up"].as_u64().unwrap_or(0)));
        out.insert("at".into(), json!(at));
        let mut q = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if q.len() == WAITING {
            q.pop_front();
        }
        q.push_back((job.to_string(), out));
    }

    /// Each line since the last call, with the job of its connection.
    pub fn take(&self) -> Vec<(String, Map<String, Value>)> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(decision: &str, stage: Option<&'static str>) -> Line {
        Line {
            schema: 1,
            time: "2026-11-18T00:00:00Z".into(),
            level: "info",
            event: "broker.connection",
            fields: json!({ "job": "u-1", "host": "github.com", "port": 443, "decision": decision,
                "rule": "allow:github.com", "up": 10, "down": 20, "path": "/secret" }),
            stage,
            class: None,
        }
    }

    /// T-486: an allowed, a denied and a failed connection give one line
    /// each, with no field of the request beyond the host and the port.
    #[test]
    fn each_connection_gives_one_line_with_no_path() {
        let l = Lines::default();
        l.keep(&line("allow", None), "2026-11-18T00:00:01Z");
        l.keep(&line("deny", None), "2026-11-18T00:00:02Z");
        l.keep(&line("allow", Some("connect")), "2026-11-18T00:00:03Z");
        let got = l.take();
        let verdicts: Vec<&str> = got.iter().map(|(_, m)| m["verdict"].as_str().unwrap()).collect();
        assert_eq!(verdicts, ["allow", "deny", "fail"]);
        assert_eq!(got[0].0, "u-1");
        assert_eq!((got[0].1["bytes_in"].as_u64(), got[0].1["bytes_out"].as_u64()), (Some(20), Some(10)));
        assert!(got.iter().all(|(_, m)| !m.contains_key("path")), "{got:?}");
        assert!(l.take().is_empty(), "taken once");
    }
}
