//! `slots`: the supervisor of podrunner (GitHub #21). It holds the state
//! of each job, places each offer on a unit kind, writes the journal and
//! the audit lines, and reports each job to its source. A restart reads
//! the journal: it destroys each unit that was not reported, releases each
//! claimed job as `lost`, and stays `degraded` while a residue is left.

pub mod audit;
pub mod config;
mod engine;
pub mod journal;
pub mod placement;
mod run;
pub mod state;

use std::path::Path;
use std::sync::Arc;

pub use engine::{Job, Slots, Source, UnitKind};
use podrunner_clock::Clock;
use podrunner_schema::{ExitReason, UnitIds};

use crate::config::Config;
use crate::state::{Line, State};

impl Slots {
    /// Start the supervisor: read the journal of `state_dir`, end what it
    /// left, and give the slots. `now_unix` gives the time of the Unix
    /// epoch in seconds.
    pub fn start(
        cfg: Config,
        state_dir: &Path,
        clock: Arc<dyn Clock>,
        now_unix: Box<dyn Fn() -> u64 + Send>,
        sources: Vec<Source>,
        kinds: Vec<UnitKind>,
    ) -> std::io::Result<Slots> {
        let replay = journal::replay(state_dir)?;
        let journal = journal::Journal::open(state_dir)?;
        let audit = audit::Audit::open(state_dir)?;
        let mut s = Slots {
            cfg,
            clock,
            now_unix,
            sources,
            kinds,
            jobs: Default::default(),
            suppressed: Default::default(),
            journal,
            audit,
            ids: UnitIds::starting_at(replay.next_unit),
            draining: false,
            degraded: None,
            reports: Vec::new(),
        };
        s.reconcile(replay.open);
        Ok(s)
    }

    /// End each job that the journal left open.
    fn reconcile(&mut self, open: std::collections::BTreeMap<(String, podrunner_schema::JobId), Line>) {
        let now = self.now();
        for ((source, job_id), line) in open {
            if let Some(unit) = &line.unit_id {
                let kind = self.kinds.iter().find(|k| Some(&k.name) == line.unit_kind.as_ref());
                let clean = match kind.map(|k| k.driver.destroy(unit)) {
                    Some(Ok(r)) => r.is_clean(),
                    // A unit whose kind is gone, or a failed destroy, is a
                    // residue that nobody counted.
                    _ => false,
                };
                if !clean {
                    self.degraded = Some(format!("the restart could not end {unit}"));
                }
            }
            let s = self.sources.iter().position(|x| x.cfg.name == source);
            if let Some(s) = s
                && line.state.is_claimed()
            {
                let _ = self.sources[s].source.release(&job_id);
                let report = podrunner_schema::RunReport {
                    schema: podrunner_schema::SCHEMA,
                    job_id: job_id.clone(),
                    source: source.clone(),
                    tier: None,
                    host: self.cfg.host.clone(),
                    exit_reason: ExitReason::Lost,
                    exit_code: None,
                    wall_s: 0,
                    refusal_code: None,
                    unit: None,
                };
                let value = if self.sources[s].cfg.operator_controlled {
                    serde_json::to_value(&report)
                } else {
                    serde_json::to_value(report.public())
                };
                if let Ok(serde_json::Value::Object(m)) = value {
                    let _ = self.sources[s].source.report(&job_id, &m);
                }
                self.reports.push(report);
            }
            let lost = Line { t_ms: now, state: State::Lost, reason: Some(ExitReason::Lost), ..line };
            if self.journal.write(&lost).is_err() {
                self.degraded = Some("the journal cannot be written".into());
            }
        }
    }
}

#[cfg(test)]
mod tests;
