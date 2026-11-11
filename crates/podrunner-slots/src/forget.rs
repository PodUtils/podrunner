//! The bounds of a long `serve` (T-068): a final job leaves memory an hour
//! after its last transition, with a suppression of its id for one more
//! hour (a redelivered offer starts no second run); an expired suppression
//! goes; the reports keep the last [`REPORTS_KEPT`]. The journal is
//! compacted at start ([`crate::journal::compact`]).

use std::collections::VecDeque;

use podrunner_schema::RunReport;

use crate::Slots;

/// How long a final job stays in memory and in `status`.
pub(crate) const FINAL_KEPT_MS: u64 = 3_600_000;

/// The count of reports that `status` and the tests can read.
pub const REPORTS_KEPT: usize = 256;

impl Slots {
    /// Forget each final job that is older than [`FINAL_KEPT_MS`], and each
    /// suppression that has passed.
    pub(crate) fn forget(&mut self, now: u64) {
        let old: Vec<_> = self
            .jobs
            .iter()
            .filter(|(_, j)| j.state.is_final() && j.since_ms + FINAL_KEPT_MS <= now)
            .map(|(k, _)| k.clone())
            .collect();
        for key in old {
            self.jobs.remove(&key);
            let until = self.suppressed.entry(key.1).or_insert(0);
            *until = (*until).max(now + FINAL_KEPT_MS);
        }
        self.suppressed.retain(|_, until| *until > now);
    }
}

/// Keep `r` in `reports`, and drop the oldest past [`REPORTS_KEPT`].
pub(crate) fn keep(reports: &mut VecDeque<RunReport>, r: RunReport) {
    reports.push_back(r);
    while reports.len() > REPORTS_KEPT {
        reports.pop_front();
    }
}
