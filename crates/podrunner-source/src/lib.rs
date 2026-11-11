//! The source seam of podrunner (GitHub #16, `docs/contracts/source-driver.md`).
//!
//! A source finds jobs: a forge, a scale set, or the `fake` source. The seam
//! is JSON lines from the first day, because the scale-set sidecar is Go
//! ([`wire`]). In the process, a source is the trait [`JobSource`];
//! [`WireSource`] speaks the wire to a source in another process, and
//! [`serve_line`] answers one line of the wire from a [`JobSource`].
//! [`conformance`] holds the tests that each source must pass.

pub mod conformance;
mod event;
mod pipe;
pub mod wire;

use std::fmt;
use std::time::Duration;

pub use event::{Event, Offer};
pub use pipe::{WireSource, serve_line};
use podrunner_schema::{Credential, JobId};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The class of a refusal of a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceCode {
    /// A line that the wire refuses: too long, not JSON, an unknown `op`, a
    /// bad `job_id`, another `schema`.
    BadRequest,
    /// No offer with this job id (never offered, withdrawn, or released).
    UnknownJob,
    /// The job is claimed already.
    AlreadyClaimed,
    /// The operation needs a claim first.
    NotClaimed,
    /// The source revoked the credential of podrunner (`source-revoked`).
    SourceRevoked,
    /// The source cannot give a credential now (`credential-unavailable`).
    CredentialUnavailable,
    /// The source is not reachable.
    Unavailable,
}

impl SourceCode {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceCode::BadRequest => "bad-request",
            SourceCode::UnknownJob => "unknown-job",
            SourceCode::AlreadyClaimed => "already-claimed",
            SourceCode::NotClaimed => "not-claimed",
            SourceCode::SourceRevoked => "source-revoked",
            SourceCode::CredentialUnavailable => "credential-unavailable",
            SourceCode::Unavailable => "unavailable",
        }
    }
}

/// A refusal of a source: a code and a detail with no credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceError {
    pub code: SourceCode,
    pub detail: String,
}

impl SourceError {
    pub fn new(code: SourceCode, detail: impl Into<String>) -> SourceError {
        SourceError { code, detail: detail.into() }
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.detail)
    }
}

impl std::error::Error for SourceError {}

/// The reply to `hello`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    /// The name of the source in the configuration and the reports.
    pub source: String,
    /// True when the operator of podrunner controls the source: it gets the
    /// full run report, else the public report.
    pub operator_controlled: bool,
}

/// A source of jobs. The order of one job: `offer` (an event of `next`),
/// `ack`, `claim`, `mint`, then `report` and `release`; `withdraw` ends an
/// offer that podrunner does not claim.
pub trait JobSource: Send {
    fn hello(&mut self, client: &str, version: &str) -> Result<Hello, SourceError>;
    /// Up to `capacity` offers, and the other events, within `wait`.
    fn next(&mut self, capacity: u32, wait: Duration) -> Result<Vec<Event>, SourceError>;
    fn ack(&mut self, job: &JobId) -> Result<(), SourceError>;
    fn claim(&mut self, job: &JobId, runner_name: &str) -> Result<(), SourceError>;
    /// The credential of the runner of a claimed job.
    fn mint(&mut self, job: &JobId, runner_name: &str) -> Result<Credential, SourceError>;
    fn report(&mut self, job: &JobId, report: &Map<String, Value>) -> Result<(), SourceError>;
    fn audit(&mut self, line: &Map<String, Value>) -> Result<(), SourceError>;
    fn withdraw(&mut self, job: &JobId, reason: &str) -> Result<(), SourceError>;
    fn release(&mut self, job: &JobId) -> Result<(), SourceError>;
}

#[cfg(test)]
mod tests;
