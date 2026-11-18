//! The JSON-lines wire of the source seam (`docs/contracts/source-driver.md`).
//! One request on one line, one reply on one line. Each object starts with
//! `"schema": 1`. A line over [`LINE_MAX`] bytes, an unknown `op`, a bad
//! `job_id` or another `schema` is `bad-request`. The form of the wire is
//! `pod_job::source` (podcrates `v0.72.0`): one form for podrunner and each
//! source in another language.

pub use pod_job::source::{LINE_MAX, OPS, Reply, Request, parse_reply, parse_request, reply_line, request_line};
