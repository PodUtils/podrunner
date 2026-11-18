//! The answer of a source to `assigned`, in the process. The events of
//! `next` and the job of the forge are `pod_job::source` (podcrates
//! `v0.72.0`).

use podrunner_schema::offer::TrustInputs;

/// The answer of a source to `assigned`: run the job with these trust
/// facts, or end it with a code.
#[derive(Debug, Clone, PartialEq)]
pub enum Assigned {
    Keep(TrustInputs),
    End { code: String, detail: String },
}
