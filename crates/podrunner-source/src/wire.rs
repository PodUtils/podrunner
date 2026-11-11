//! The JSON-lines wire of the source seam (`docs/contracts/source-driver.md`).
//! One request on one line, one reply on one line. Each object starts with
//! `"schema": 1`. A line over [`LINE_MAX`] bytes, an unknown `op`, a bad
//! `job_id` or another `schema` is `bad-request`.

use podrunner_schema::{Credential, JobId, SCHEMA};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Event, SourceCode, SourceError};

/// The longest line of the wire, without its end of line.
pub const LINE_MAX: usize = 1 << 20;

/// A request of podrunner to a source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Request {
    Hello {
        client: String,
        version: String,
    },
    /// A long poll: up to `capacity` offers, within `wait_s` seconds.
    Next {
        capacity: u32,
        wait_s: u32,
    },
    Ack {
        job_id: JobId,
    },
    Claim {
        job_id: JobId,
        runner_name: String,
    },
    Mint {
        job_id: JobId,
        runner_name: String,
    },
    Report {
        job_id: JobId,
        report: Map<String, Value>,
    },
    Audit {
        line: Map<String, Value>,
    },
    Withdraw {
        job_id: JobId,
        reason: String,
    },
    Release {
        job_id: JobId,
    },
}

/// The names of the operations, in the order of the contract.
pub const OPS: [&str; 9] = ["hello", "next", "ack", "claim", "mint", "report", "audit", "withdraw", "release"];

impl Request {
    pub fn op(&self) -> &'static str {
        match self {
            Request::Hello { .. } => "hello",
            Request::Next { .. } => "next",
            Request::Ack { .. } => "ack",
            Request::Claim { .. } => "claim",
            Request::Mint { .. } => "mint",
            Request::Report { .. } => "report",
            Request::Audit { .. } => "audit",
            Request::Withdraw { .. } => "withdraw",
            Request::Release { .. } => "release",
        }
    }
}

/// The reply of a source to one request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Reply {
    /// The reply to `hello`. A source that is not `operator_controlled`
    /// gets only the public report.
    Hello {
        source: String,
        operator_controlled: bool,
    },
    /// The reply to `next`.
    Events {
        events: Vec<Event>,
    },
    /// The reply to `mint`.
    Credential {
        credential: Credential,
    },
    /// The reply to each other request.
    Done,
    Error {
        code: SourceCode,
        detail: String,
    },
}

/// An object of the wire with its schema.
#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    schema: u32,
    #[serde(flatten)]
    body: T,
}

fn bad(detail: impl Into<String>) -> SourceError {
    SourceError::new(SourceCode::BadRequest, detail)
}

/// Read one line of the wire. The error detail holds no byte of the line:
/// a line can hold a credential.
fn parse<T: for<'de> Deserialize<'de>>(line: &[u8]) -> Result<T, SourceError> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    if line.len() > LINE_MAX {
        return Err(bad(format!("a line of {} bytes, over {LINE_MAX}", line.len())));
    }
    let v: Value = serde_json::from_slice(line).map_err(|e| bad(format!("not JSON: {}", category(&e))))?;
    match v.get("schema") {
        Some(Value::Number(n)) if n.as_u64() == Some(SCHEMA as u64) => {}
        _ => return Err(bad(format!("no \"schema\": {SCHEMA}"))),
    }
    let e: Envelope<T> = serde_json::from_value(v).map_err(|e| bad(format!("refused: {}", short(&e.to_string()))))?;
    Ok(e.body)
}

/// A serde error with each quoted text removed: serde can quote a value of
/// the line, and a value can be a credential. The words that stay name the
/// class of the fault and the position.
fn short(s: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match quote {
            Some(q) if c == q => {
                out.push(c);
                quote = None;
            }
            Some(_) => {}
            None if c == '"' || c == '`' => {
                out.push(c);
                out.push_str("...");
                quote = Some(c);
            }
            None => out.push(c),
        }
    }
    out.chars().take(200).collect()
}

/// The class of a JSON error, with no byte of the line.
fn category(e: &serde_json::Error) -> &'static str {
    match e.classify() {
        serde_json::error::Category::Io => "io",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "data",
        serde_json::error::Category::Eof => "end of line",
    }
}

pub fn parse_request(line: &[u8]) -> Result<Request, SourceError> {
    parse(line)
}

pub fn parse_reply(line: &[u8]) -> Result<Reply, SourceError> {
    parse(line)
}

fn write<T: Serialize>(body: &T) -> String {
    let mut s = serde_json::to_string(&Envelope { schema: SCHEMA, body }).expect("a wire object is JSON");
    s.push('\n');
    s
}

/// One line of a request, with its end of line.
pub fn request_line(r: &Request) -> String {
    write(r)
}

/// One line of a reply, with its end of line.
pub fn reply_line(r: &Reply) -> String {
    write(r)
}
