//! The spool of the reports (T-069, SPEC §11.1): a report that the source
//! refuses goes to `STATE/spool/` (mode 0600, the report only, no
//! credential), with an audit line `report-spooled`. Each spooled report
//! goes to its source again every [`RETRY_MS`], first at the start; the
//! file goes when the source takes it. A sidecar that restarts, or a mint
//! that the host cannot reach for a time, loses no report.

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use podrunner_schema::JobId;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Slots, audit};

/// The time between two sends of the spooled reports.
const RETRY_MS: u64 = 30_000;

#[derive(Serialize, Deserialize)]
struct Spooled {
    source: String,
    job_id: JobId,
    report: Map<String, Value>,
}

impl Slots {
    fn spool_dir(&self) -> PathBuf {
        self.state_dir.join("spool")
    }

    /// Send `report` of `job` to the source `s`; spool it on a refusal.
    pub(crate) fn send_report(&mut self, s: usize, job: &JobId, report: Map<String, Value>) {
        if self.sources[s].source.report(job, &report).is_ok() {
            return;
        }
        let entry = Spooled { source: self.sources[s].cfg.name.clone(), job_id: job.clone(), report };
        if self.spool(&entry).is_err() {
            self.degraded = Some("a refused report cannot be spooled".into());
        }
        self.audit(audit::Kind::ReportSpooled, s, job, None, None);
    }

    fn spool(&self, entry: &Spooled) -> std::io::Result<()> {
        let dir = self.spool_dir();
        std::fs::create_dir_all(&dir)?;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        // Source names and job ids are plain words (the configuration and
        // pod-job check them); a byte outside of them becomes `_`.
        let plain = |t: &str| {
            t.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect::<String>()
        };
        let name = format!("{}--{}.json", plain(&entry.source), plain(entry.job_id.as_str()));
        let part = dir.join(format!("{name}.part"));
        let mut f = std::fs::OpenOptions::new().create(true).write(true).truncate(true).mode(0o600).open(&part)?;
        f.write_all(&serde_json::to_vec(entry).map_err(std::io::Error::other)?)?;
        f.sync_all()?;
        std::fs::rename(&part, dir.join(name))
    }

    /// Send each spooled report again when its time comes.
    pub(crate) fn retry_spool(&mut self, now: u64) {
        if now < self.spool_next_ms {
            return;
        }
        self.spool_next_ms = now + RETRY_MS;
        let Ok(dir) = std::fs::read_dir(self.spool_dir()) else { return };
        for path in dir.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")) {
            let Some(entry) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Spooled>(&b).ok()) else {
                continue;
            };
            let Some(s) = self.sources.iter().position(|x| x.cfg.name == entry.source) else { continue };
            if self.sources[s].source.report(&entry.job_id, &entry.report).is_ok() {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    /// The count of the reports in the spool.
    pub fn spooled(&self) -> usize {
        std::fs::read_dir(self.spool_dir())
            .map(|d| d.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "json")).count())
            .unwrap_or(0)
    }
}
