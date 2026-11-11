//! The offer ledger of the remote mint (GitHub #31): which host holds each
//! offer, and which job has its credential. Each change is a JSON line,
//! written and synced to disk before the mint answers; a start reads the
//! lines again, then writes the ledger again with what a later call can
//! need (T-068): the open offers, the credentials with no start, and each
//! credential of the last [`MINTED_KEPT_S`] (the gate refuses a second
//! one for its job).

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use podrunner_schema::JobId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
enum Line {
    Offer {
        host: String,
        job: JobId,
    },
    Mint {
        host: String,
        job: JobId,
        #[serde(default)]
        t_s: u64,
    },
    /// The source said that the runner of the job started.
    Start {
        host: String,
        job: JobId,
    },
    End {
        host: String,
        job: JobId,
    },
}

/// How long the ledger keeps a credential of a job with no open offer: a
/// source that offers the job again within it gets no second credential.
pub const MINTED_KEPT_S: u64 = 7 * 24 * 3600;

/// The ledger.
pub struct Ledger {
    file: File,
    /// The open offers of each host.
    offers: BTreeMap<String, BTreeSet<JobId>>,
    /// The jobs with a credential, with the host and the time of the
    /// mint: never a second one.
    minted: BTreeMap<JobId, (String, u64)>,
    /// The credentials with no start yet: the host and the time of the mint.
    pending: BTreeMap<JobId, (String, u64)>,
}

impl Ledger {
    /// Open the ledger at `path` (mode 0600), read its lines, and compact
    /// it at the time of the system.
    pub fn open(path: &Path) -> std::io::Result<Ledger> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        Ledger::open_at(path, now)
    }

    /// [`Ledger::open`] at the time `now_s` (Unix seconds).
    pub fn open_at(path: &Path, now_s: u64) -> std::io::Result<Ledger> {
        use std::os::unix::fs::OpenOptionsExt;
        let file = OpenOptions::new().create(true).append(true).read(true).mode(0o600).open(path)?;
        let mut l = Ledger { file, offers: BTreeMap::new(), minted: BTreeMap::new(), pending: BTreeMap::new() };
        for line in BufReader::new(File::open(path)?).lines() {
            // A torn last line (a crash in a write) is skipped.
            if let Ok(line) = serde_json::from_str::<Line>(&line?) {
                l.apply(&line);
            }
        }
        l.compact(path, now_s)?;
        Ok(l)
    }

    /// Write the ledger again with the lines that a later call can need,
    /// beside the old file, synced, then renamed over it.
    fn compact(&mut self, path: &Path, now_s: u64) -> std::io::Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        let offered: BTreeSet<&JobId> = self.offers.values().flatten().collect();
        self.minted.retain(|job, (_, t)| offered.contains(job) || t.saturating_add(MINTED_KEPT_S) > now_s);
        let mut lines = Vec::new();
        for (host, jobs) in &self.offers {
            lines.extend(jobs.iter().map(|job| Line::Offer { host: host.clone(), job: job.clone() }));
        }
        for (job, (host, t)) in &self.minted {
            lines.push(Line::Mint { host: host.clone(), job: job.clone(), t_s: *t });
            if !self.pending.contains_key(job) {
                lines.push(Line::Start { host: host.clone(), job: job.clone() });
            }
        }
        let part = path.with_extension("jsonl.part");
        let mut f = OpenOptions::new().create(true).write(true).truncate(true).mode(0o600).open(&part)?;
        for line in &lines {
            let mut text = serde_json::to_string(line).map_err(std::io::Error::other)?;
            text.push('\n');
            f.write_all(text.as_bytes())?;
        }
        f.sync_all()?;
        std::fs::rename(&part, path)?;
        self.file = OpenOptions::new().append(true).open(path)?;
        Ok(())
    }

    fn apply(&mut self, line: &Line) {
        match line {
            Line::Offer { host, job } => {
                self.offers.entry(host.clone()).or_default().insert(job.clone());
            }
            Line::Mint { host, job, t_s } => {
                self.minted.insert(job.clone(), (host.clone(), *t_s));
                self.pending.insert(job.clone(), (host.clone(), *t_s));
            }
            Line::Start { job, .. } => {
                self.pending.remove(job);
            }
            Line::End { host, job } => {
                if let Some(o) = self.offers.get_mut(host) {
                    o.remove(job);
                }
                self.pending.remove(job);
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

    /// The open offers of `host`, by job id.
    pub fn offers_of(&self, host: &str) -> Vec<JobId> {
        self.offers.get(host).map(|o| o.iter().cloned().collect()).unwrap_or_default()
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
        self.minted.contains_key(job)
    }

    /// Record the credential of `job` at `t_s`; false when it has one
    /// already.
    pub fn mint(&mut self, host: &str, job: &JobId, t_s: u64) -> std::io::Result<bool> {
        if self.minted.contains_key(job) {
            return Ok(false);
        }
        self.write(Line::Mint { host: host.into(), job: job.clone(), t_s })?;
        Ok(true)
    }

    /// The runner of `job` started: its credential is used.
    pub fn start(&mut self, host: &str, job: &JobId) -> std::io::Result<()> {
        if self.pending.contains_key(job) {
            self.write(Line::Start { host: host.into(), job: job.clone() })?;
        }
        Ok(())
    }

    /// The credentials with no start: the job, its host, the time of the
    /// mint.
    pub fn pending(&self) -> impl Iterator<Item = (&JobId, &str, u64)> {
        self.pending.iter().map(|(j, (h, t))| (j, h.as_str(), *t))
    }

    pub fn end(&mut self, host: &str, job: &JobId) -> std::io::Result<()> {
        if self.offers.get(host).is_some_and(|o| o.contains(job)) {
            self.write(Line::End { host: host.into(), job: job.clone() })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-068: a reopen keeps the open offers, the pending credentials and
    /// each recent credential, and drops the rest.
    #[test]
    fn ledger_compact_keeps_what_a_later_call_needs() {
        let dir = std::env::temp_dir().join(format!("podrunner-ledger-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ledger.jsonl");
        let j = |n: u32| JobId::new(&format!("j-{n}")).unwrap();
        let mut l = Ledger::open_at(&path, 0).unwrap();
        for n in 0..100 {
            l.offer("h1", &j(n)).unwrap();
            l.mint("h1", &j(n), 10).unwrap();
            l.start("h1", &j(n)).unwrap();
            l.end("h1", &j(n)).unwrap();
        }
        l.offer("h1", &j(200)).unwrap(); // open, no credential
        l.offer("h1", &j(201)).unwrap();
        l.mint("h1", &j(201), 10).unwrap(); // open, credential with no start
        drop(l);
        let before = std::fs::read_to_string(&path).unwrap().lines().count();
        // Within the time: each credential stays, so no job gets a second.
        let l = Ledger::open_at(&path, 1000).unwrap();
        assert!(l.minted(&j(0)) && l.minted(&j(201)));
        assert_eq!(l.offers_of("h1"), vec![j(200), j(201)]);
        assert_eq!(l.pending().count(), 1);
        drop(l);
        // Past it: only the open offers and their credentials stay.
        let l = Ledger::open_at(&path, 10 + MINTED_KEPT_S).unwrap();
        assert!(!l.minted(&j(0)) && l.minted(&j(201)));
        assert_eq!(l.offers_of("h1"), vec![j(200), j(201)]);
        assert_eq!(l.pending().collect::<Vec<_>>(), vec![(&j(201), "h1", 10)]);
        drop(l);
        let after = std::fs::read_to_string(&path).unwrap().lines().count();
        assert!(after == 3 && before > 400, "{before} lines, then {after}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
