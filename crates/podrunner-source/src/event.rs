//! The events of `next`.

use podrunner_schema::JobId;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A job offer. The fields other than `job_id` are those of `job-offer.v1`
/// (GitHub #17); T-017 gives them their types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Offer {
    pub job_id: JobId,
    #[serde(flatten)]
    pub fields: Map<String, Value>,
}

/// An event of a source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Event {
    /// A job that podrunner can take.
    Offer { offer: Offer },
    /// The forge cancelled a job.
    Cancel { job_id: JobId },
    /// The forge says that a job ended.
    Completed { job_id: JobId, result: String },
    /// The forge says that a runner took a job. podrunner compares the
    /// runner name with the one of the unit (`job-mismatch`).
    Started { job_id: JobId, runner_name: String },
}

impl Event {
    pub fn job_id(&self) -> &JobId {
        match self {
            Event::Offer { offer } => &offer.job_id,
            Event::Cancel { job_id } | Event::Completed { job_id, .. } | Event::Started { job_id, .. } => job_id,
        }
    }
}
