//! The disk limit of a unit (T-074, T-652). A thread measures the roots
//! of live units off the tick. The tick ends a unit above its bound with
//! an audit line `limit` (`disk`) and the exit reason `limit`. The bound
//! is the lesser of the `disk` limit of the offer and `[unit.X]
//! disk_max_bytes` when both are set.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use podrunner_schema::{ExitReason, JobId, UnitId};
use podrunner_unit::Signal;

use crate::state::State;
use crate::{Job, Slots, audit};

/// The time between two measures of the disk of the units.
pub const CHECK_MS: u64 = 30_000;

/// The bytes of the files under `root`, on its file system, with no link
/// followed.
pub fn bytes_under(root: &Path) -> u64 {
    bytes_under_to(root, u64::MAX)
}

/// The bytes read, with the walk stopped once the count is above `bound`.
fn bytes_under_to(root: &Path, bound: u64) -> u64 {
    let Ok(top) = root.symlink_metadata() else { return 0 };
    if !top.is_dir() {
        return 0;
    }
    let dev = top.dev();
    let mut total = 0u64;
    let mut todo = vec![root.to_path_buf()];
    while let Some(dir) = todo.pop() {
        let Ok(m) = dir.symlink_metadata() else { continue };
        if !m.is_dir() || m.dev() != dev {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.dev() != dev {
                continue;
            }
            if m.is_dir() {
                todo.push(e.path());
            } else {
                total = total.saturating_add(m.len());
                if total > bound {
                    return total;
                }
            }
        }
    }
    total
}

type Key = (usize, JobId);

struct Task {
    key: Key,
    unit: UnitId,
    root: PathBuf,
    bound: u64,
}

struct Sample {
    key: Key,
    unit: UnitId,
    bound: u64,
    bytes: u64,
}

/// One batch of disk walks at a time.
pub(crate) struct Worker {
    tasks: mpsc::Sender<Vec<Task>>,
    results: mpsc::Receiver<Sample>,
    next_ms: u64,
    pub(crate) pending: usize,
    #[cfg(test)]
    pub(crate) gate: std::sync::Arc<std::sync::Mutex<()>>,
}

impl Worker {
    pub(crate) fn start() -> std::io::Result<Self> {
        let (tasks, batches) = mpsc::channel::<Vec<Task>>();
        let (done, results) = mpsc::channel();
        #[cfg(test)]
        let gate = std::sync::Arc::new(std::sync::Mutex::new(()));
        #[cfg(test)]
        let held = gate.clone();
        std::thread::Builder::new().name("podrunner-disk".into()).spawn(move || {
            for batch in batches {
                for Task { key, unit, root, bound } in batch {
                    #[cfg(test)]
                    let _held = held.lock();
                    let bytes = bytes_under_to(&root, bound);
                    if done.send(Sample { key, unit, bound, bytes }).is_err() {
                        return;
                    }
                }
            }
        })?;
        Ok(Self {
            tasks,
            results,
            next_ms: 0,
            pending: 0,
            #[cfg(test)]
            gate,
        })
    }
}

fn live(job: &Job) -> bool {
    matches!(job.state, State::Started | State::Running) && job.term_ms.is_none()
}

impl Slots {
    /// Read the samples that are back. Send one batch when it is due and
    /// the last batch is done.
    pub(crate) fn check_disks(&mut self, now: u64) {
        while let Ok(sample) = self.disk_worker.results.try_recv() {
            self.disk_worker.pending -= 1;
            self.disk_sample(sample, now);
        }
        if now < self.disk_worker.next_ms || self.disk_worker.pending != 0 {
            return;
        }
        self.disk_worker.next_ms = now.saturating_add(CHECK_MS);
        let mut batch = Vec::new();
        for (key, j) in self.jobs.iter().filter(|(_, j)| live(j)) {
            let (Some(unit), Some(p)) = (&j.unit, &j.placement) else { continue };
            let kind = &self.kinds[p.kind];
            // The lesser of the offer and the kind, as for each other limit
            // (T-198): an offer only narrows the bound of the operator.
            let offered = j.offer.as_ref().and_then(|o| o.limits.get("disk").copied());
            let bound = match (offered, kind.cfg.disk_max_bytes) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            let Some(bound) = bound else { continue };
            let Some(root) = kind.driver.observe(unit).ok().and_then(|o| o.unit_root) else { continue };
            batch.push(Task { key: key.clone(), unit: unit.clone(), root, bound });
        }
        let pending = batch.len();
        if pending != 0 && self.disk_worker.tasks.send(batch).is_ok() {
            self.disk_worker.pending = pending;
        }
    }

    fn disk_sample(&mut self, Sample { key, unit, bound, bytes }: Sample, now: u64) {
        let Some(j) = self.jobs.get_mut(&key) else { return };
        if !live(j) || j.unit.as_ref() != Some(&unit) {
            return;
        }
        j.times.disk_peak = Some(j.times.disk_peak.map_or(bytes, |p| p.max(bytes)));
        if bytes > bound {
            self.audit(audit::Kind::Limit, key.0, &key.1, None, Some("disk".into()));
            self.end(&key, ExitReason::Limit, Signal::Term, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;

    use super::*;
    use crate::tests::TempDir;

    #[test]
    fn a_bounded_walk_stops_above_the_bound() {
        let dir = TempDir::new();
        for i in 0..3 {
            fs::write(dir.0.join(i.to_string()), b"123").unwrap();
        }
        assert_eq!(bytes_under_to(&dir.0, 3), 6);
        assert_eq!(bytes_under_to(&dir.0, 9), 9);
        assert_eq!(bytes_under(&dir.0), 9);
    }

    #[test]
    fn the_walk_counts_a_link_without_reading_its_target() {
        let dir = TempDir::new();
        let root = dir.0.join("root");
        fs::create_dir(&root).unwrap();
        let outside = dir.0.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("file"), [0; 1024]).unwrap();
        let link = root.join("link");
        symlink(&outside, &link).unwrap();
        assert_eq!(bytes_under(&root), link.symlink_metadata().unwrap().len());
        assert_eq!(bytes_under(&link), 0);
    }
}
