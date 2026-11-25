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

use std::time::Duration;

pub use event::Assigned;
pub use pipe::{Exchange, Stream, Wire, WireSource, serve_line};
pub use pod_job::source::{Event, HostFacts, KindFacts, Offer, REF_MAX, RealJob, SourceCode, SourceError, ref_fault};
use podrunner_schema::{Credential, JobId};
use serde_json::{Map, Value};

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
    /// `hello` with the facts of the host (T-495). A source in this process
    /// needs no facts: the default is `hello`.
    fn hello_host(&mut self, client: &str, version: &str, host: &HostFacts) -> Result<Hello, SourceError> {
        let _ = host;
        self.hello(client, version)
    }
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
    /// The forge gave the runner of the claimed `job` the job `real`, not
    /// the run of the offer (T-493): keep it, with its trust facts, or end
    /// it. A source with no such operation refuses it as `bad-request`, and
    /// podrunner then ends the job.
    fn assigned(&mut self, job: &JobId, real: &RealJob) -> Result<Assigned, SourceError> {
        let _ = (job, real);
        Err(SourceError::new(SourceCode::BadRequest, "assigned is not an operation of this source"))
    }
}

#[cfg(test)]
mod tests;
