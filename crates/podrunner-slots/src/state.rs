//! The states of a job in podrunner, and the journal line of each
//! transition.

use podrunner_schema::{ExitReason, JobId, RefusalCode, UnitId};
use serde::{Deserialize, Serialize};

/// The state of a job. The main path runs from `offered` to `reported`;
/// `refused`, `released`, `failed` and `lost` end it on another path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    Offered,
    Matched,
    Claimed,
    Minted,
    Started,
    Running,
    Ended,
    Destroyed,
    Reported,
    Refused,
    Released,
    Failed,
    Lost,
}

impl State {
    /// A state after which nothing happens to the job.
    pub fn is_final(self) -> bool {
        matches!(self, State::Reported | State::Refused | State::Released | State::Failed | State::Lost)
    }

    /// From `claimed` on, the source holds the job for podrunner, or may
    /// hold it: the line is written before the claim at the source
    /// (GitHub #55). A restart releases it and reports `lost`.
    pub fn is_claimed(self) -> bool {
        matches!(
            self,
            State::Claimed | State::Minted | State::Started | State::Running | State::Ended | State::Destroyed
        )
    }

    /// The key of the deadline of the state in `[deadlines]`, if it has one.
    pub fn deadline_key(self) -> Option<&'static str> {
        Some(match self {
            State::Matched => "claim_s",
            State::Claimed => "mint_s",
            State::Minted => "start_s",
            State::Started => "start_s",
            State::Running => "running_s",
            State::Ended => "destroy_s",
            State::Destroyed => "report_s",
            _ => return None,
        })
    }
}

/// One line of the journal: one transition. It holds no credential, no
/// path and no argv.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Line {
    /// The time of the clock, in milliseconds.
    pub t_ms: u64,
    pub source: String,
    pub job_id: JobId,
    pub state: State,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_id: Option<UnitId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<ExitReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<RefusalCode>,
    /// The exit code of the top process of the unit, once it ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}
