//! The offer ledger of the remote mint (GitHub #31): which host holds each
//! offer, and which job has its credential. Each change is a JSON line,
//! written and synced to disk before the mint answers; a start reads the
//! lines again.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use podrunner_schema::JobId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
enum Line {
    Offer { host: String, job: JobId },
    Mint { host: String, job: JobId },
    End { host: String, job: JobId },
}

/// The ledger.
pub struct Ledger {
    file: File,
    /// The open offers of each host.
    offers: BTreeMap<String, BTreeSet<JobId>>,
    /// The jobs with a credential: never a second one.
    minted: BTreeSet<JobId>,
}

impl Ledger {
    /// Open the ledger at `path` (mode 0600) and read its lines.
    pub fn open(path: &Path) -> std::io::Result<Ledger> {
        use std::os::unix::fs::OpenOptionsExt;
        let file = OpenOptions::new().create(true).append(true).read(true).mode(0o600).open(path)?;
        let mut l = Ledger { file: file.try_clone()?, offers: BTreeMap::new(), minted: BTreeSet::new() };
        for line in BufReader::new(file).lines() {
            // A torn last line (a crash in a write) is skipped.
            if let Ok(line) = serde_json::from_str::<Line>(&line?) {
                l.apply(&line);
            }
        }
        Ok(l)
    }

    fn apply(&mut self, line: &Line) {
        match line {
            Line::Offer { host, job } => {
                self.offers.entry(host.clone()).or_default().insert(job.clone());
            }
            Line::Mint { job, .. } => {
                self.minted.insert(job.clone());
            }
            Line::End { host, job } => {
                if let Some(o) = self.offers.get_mut(host) {
                    o.remove(job);
                }
            }
        }
    }

    fn write(&mut self, line: Line) -> std::io::Result<()> {
        let mut text = serde_json::to_string(&line).map_err(std::io::Error::other)?;
        text.push('\n');
        self.file.write_all(text.as_bytes())?;
        self.file.sync_data()?;
        self.apply(&line);
        Ok(())
    }

    /// The host that holds the offer of `job`.
    pub fn holder(&self, job: &JobId) -> Option<&str> {
        self.offers.iter().find(|(_, o)| o.contains(job)).map(|(h, _)| h.as_str())
    }

    /// The open offers of `host`.
    pub fn outstanding(&self, host: &str) -> usize {
        self.offers.get(host).map_or(0, BTreeSet::len)
    }

    pub fn offer(&mut self, host: &str, job: &JobId) -> std::io::Result<()> {
        self.write(Line::Offer { host: host.into(), job: job.clone() })
    }

    /// True when `job` has its credential.
    pub fn minted(&self, job: &JobId) -> bool {
        self.minted.contains(job)
    }

    /// Record the credential of `job`; false when it has one already.
    pub fn mint(&mut self, host: &str, job: &JobId) -> std::io::Result<bool> {
        if self.minted.contains(job) {
            return Ok(false);
        }
        self.write(Line::Mint { host: host.into(), job: job.clone() })?;
        Ok(true)
    }

    pub fn end(&mut self, host: &str, job: &JobId) -> std::io::Result<()> {
        if self.offers.get(host).is_some_and(|o| o.contains(job)) {
            self.write(Line::End { host: host.into(), job: job.clone() })?;
        }
        Ok(())
    }
}
