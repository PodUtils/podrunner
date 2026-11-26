//! The disk limit of a unit (T-074, T-652). A thread measures the roots
//! of live units off the tick. The tick ends a unit above its bound with
//! an audit line `limit` (`disk`) and the exit reason `limit`. The bound
//! is the lesser of the `disk` limit of the offer and `[unit.X]
//! disk_max_bytes` when both are set.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use podrunner_schema::{ExitReason, JobId, UnitId};
use podrunner_unit::Signal;

use crate::state::State;
use crate::{Job, Slots, audit, fdwalk};

/// The time between two measures of the disk of the units.
pub const CHECK_MS: u64 = 30_000;

/// The bytes of the files under `root`, on its file system, with no link
/// followed.
pub fn bytes_under(root: &Path) -> u64 {
    bytes_under_to(root, u64::MAX)
}

/// The bytes read, with the walk stopped once the count is above `bound`.
/// The walk opens each directory by the descriptor of its parent (T-678).
/// A walk that a job made lose its place runs once more; a second loss
/// gives a count above the bound: the limit fails closed.
fn bytes_under_to(root: &Path, bound: u64) -> u64 {
    bytes_with(root, bound, fdwalk::MAX_OPEN, |_| {})
}

/// `bytes_under_to` with a bound of descriptors, and `d` called with the
/// name of each directory before the walk reads it.
fn bytes_with(root: &Path, bound: u64, max_open: usize, mut d: impl FnMut(&std::ffi::CStr)) -> u64 {
    for _ in 0..2 {
        let mut total = 0u64;
        let opts = fdwalk::Opts { same_dev: true, max_open };
        let count = |st: &libc::stat| {
            total = total.saturating_add(u64::try_from(st.st_size).unwrap_or(0));
            total <= bound
        };
        match fdwalk::walk(root, opts, count, |_, name| d(name)).0 {
            fdwalk::End::Lost => continue,
            fdwalk::End::Done | fdwalk::End::Stopped => return total,
        }
    }
    bound.saturating_add(1)
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

    /// T-678: a job that makes each walk lose its place gets a count
    /// above the bound (the limit fails closed); one lost walk is run again.
    #[test]
    fn a_walk_lost_twice_gives_a_count_above_the_bound() {
        let dir = TempDir::new();
        let root = dir.0.join("root");
        fs::create_dir_all(root.join("a/b/c/d")).unwrap();
        fs::write(root.join("a/file"), [0; 7]).unwrap();
        // At each visit of `d`, move an ancestor that the walk closed (one
        // descriptor): it cannot check its way back up, and ends lost.
        let mut lost = 0;
        let got = bytes_with(&root, 100, 1, |name| {
            if name.to_bytes() == b"d" {
                lost += 1;
                // Pass 1: `a/b` goes up to the root; pass 2: back under `a`.
                let (from, to) = if lost == 1 {
                    (root.join("a/b"), root.join("moved1"))
                } else {
                    (root.join("moved1"), root.join("a/moved2"))
                };
                fs::rename(from, to).unwrap();
            }
        });
        assert_eq!(lost, 2, "the walk runs once more after a loss");
        assert_eq!(got, 101);
        // A walk lost once and then whole gives the true count.
        let dir = TempDir::new();
        let root = dir.0.join("root");
        fs::create_dir_all(root.join("a/b/c/d")).unwrap();
        fs::write(root.join("a/file"), [0; 7]).unwrap();
        let mut once = false;
        let got = bytes_with(&root, 100, 1, |name| {
            if name.to_bytes() == b"d" && !once {
                once = true;
                fs::rename(root.join("a/b"), root.join("moved")).unwrap();
            }
        });
        assert_eq!(got, 7);
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
