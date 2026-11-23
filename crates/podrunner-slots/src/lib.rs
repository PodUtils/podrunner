//! `slots`: the supervisor of podrunner (GitHub #21). It holds the state
//! of each job, places each offer on a unit kind, writes the journal and
//! the audit lines, and reports each job to its source. A restart reads
//! the journal: it destroys each unit that was not reported, releases each
//! claimed job as `lost`, and stays `degraded` while a residue is left.

pub mod assigned;
pub mod audit;
pub mod config;
pub mod disk;
mod driver;
pub mod egress_lines;
mod egress_port;
mod engine;
mod floors;
pub mod forget;
pub mod guest;
pub mod hooks;
mod host;
pub mod journal;
pub mod limits;
pub mod log_tail;
mod own;
pub mod pins;
pub mod placement;
pub mod policy;
mod report;
mod run;
pub mod sandbox;
pub mod secrets;
mod sources;
mod spool;
pub mod state;
pub mod status;
pub mod trust;
mod unit_config;
pub mod usage;
mod work_folder;

use std::path::Path;
use std::sync::Arc;

pub use egress_port::{JavaOptions, Opened};
pub use engine::{EgressPort, Job, Slots, Source, UnitKind};
use pod_core::clock::Clock;
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
        if replay.bad_lines > 0 {
            let kept = journal::keep_bad(state_dir, &replay).map_or("no copy".into(), |p| p.display().to_string());
            eprintln!(
                "podrunner: the journal had {} lines that could not be read; they are in {kept}",
                replay.bad_lines
            );
        }
        // The lock of serve is held: no other writer (T-068).
        journal::compact(state_dir, &replay)?;
        let journal = journal::Journal::open(state_dir)?;
        let audit = audit::Audit::open(state_dir)?;
        let mut s = Slots {
            cfg,
            origin: clock.now(),
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
            drained: Default::default(),
            stopped: Default::default(),
            degraded: None,
            reports: Default::default(),
            state_dir: state_dir.to_path_buf(),
            egress: None,
            egress_down: false,
            guest: Default::default(),
            hooks: None,
            spool_next_ms: 0,
            disk_next_ms: 0,
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
                let _ = std::fs::remove_dir_all(run::socket_dir(&self.state_dir, unit));
            }
            let s = self.sources.iter().position(|x| x.cfg.name == source);
            if let Some(s) = s
                && line.state.is_claimed()
            {
                let report = crate::report::base(job_id.clone(), source.clone(), self.cfg.host.clone());
                let value = if self.sources[s].cfg.operator_controlled {
                    serde_json::to_value(&report)
                } else {
                    serde_json::to_value(report.public())
                };
                if let Ok(serde_json::Value::Object(m)) = value {
                    self.send_report(s, &job_id, m);
                }
                // The report first: at the release the source knows the job is lost (T-200).
                let _ = self.sources[s].source.release(&job_id);
                forget::keep(&mut self.reports, report);
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
