//! The audit lines: one JSON object for each decision about a job, in
//! `STATE_DIR/audit.jsonl`, and forwarded to the source of the job (G-04).
//! A line holds no path, no query, no header and no credential.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

use podrunner_schema::{JobId, RefusalCode};
use serde::Serialize;
use serde_json::{Map, Value};

/// The kinds of audit lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Offer,
    Tier,
    Refuse,
    Claim,
    Mint,
    Sign,
    Control,
    HookRefuse,
    JobMismatch,
    PayloadIgnored,
    Destroy,
}

/// One audit line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub t_ms: u64,
    pub audit: Kind,
    pub source: String,
    pub job_id: JobId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<RefusalCode>,
    /// A short word with no data of the job (a tier, a unit kind).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Entry {
    pub fn to_map(&self) -> Map<String, Value> {
        match serde_json::to_value(self) {
            Ok(Value::Object(m)) => m,
            _ => Map::new(),
        }
    }
}

/// The writer of the audit file.
pub struct Audit {
    file: File,
    /// Each line of this process, for `status` and the tests.
    pub lines: Vec<Entry>,
}

impl Audit {
    pub fn open(state_dir: &Path) -> std::io::Result<Audit> {
        let file = OpenOptions::new().create(true).append(true).open(state_dir.join("audit.jsonl"))?;
        Ok(Audit { file, lines: Vec::new() })
    }

    pub fn write(&mut self, e: Entry) {
        if let Ok(mut text) = serde_json::to_string(&e) {
            text.push('\n');
            // An audit line that cannot be written must not stop a job:
            // the journal holds the transitions.
            let _ = self.file.write_all(text.as_bytes());
        }
        self.lines.push(e);
    }
}
