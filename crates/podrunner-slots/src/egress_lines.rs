//! The egress lines of a unit go to the source of its job (T-486,
//! githome's G-04): `{ job_id, host, port, verdict, rule, bytes_in,
//! bytes_out, at }` for each connection, at most [`PER_JOB`] for a job, then
//! one line `{ job_id, dropped }` when the job is final.

use serde_json::json;

use crate::engine::Slots;

/// The most egress lines of one job at its source.
pub const PER_JOB: u32 = 1000;

/// The egress lines of a job that went to its source, and those over the
/// bound.
#[derive(Debug, Clone, Default)]
pub struct EgressCount {
    pub sent: u32,
    pub dropped: u32,
    /// The line of the dropped count went.
    pub closed: bool,
}

impl Slots {
    /// Give each egress line since the last tick to the source of its job.
    pub(crate) fn forward_egress(&mut self) {
        let lines = self.egress.as_ref().map(|e| e.lines()).unwrap_or_default();
        for (unit, mut line) in lines {
            let key = self
                .jobs
                .iter()
                .find(|(_, j)| j.unit.as_ref().is_some_and(|u| u.as_str() == unit))
                .map(|(k, _)| k.clone());
            let Some(key) = key else { continue };
            let Some(j) = self.jobs.get_mut(&key) else { continue };
            if j.egress.sent >= PER_JOB {
                j.egress.dropped += 1;
                continue;
            }
            j.egress.sent += 1;
            line.insert("job_id".into(), json!(key.1.as_str()));
            let _ = self.sources[key.0].source.audit(&line);
        }
        let over: Vec<_> = self
            .jobs
            .iter()
            .filter(|(_, j)| j.state.is_final() && j.egress.dropped > 0 && !j.egress.closed)
            .map(|(k, j)| (k.clone(), j.egress.dropped))
            .collect();
        for (key, dropped) in over {
            let line = json!({ "job_id": key.1.as_str(), "dropped": dropped });
            if let (Some(map), Some(j)) = (line.as_object(), self.jobs.get_mut(&key)) {
                j.egress.closed = true;
                let _ = self.sources[key.0].source.audit(map);
            }
        }
    }
}
