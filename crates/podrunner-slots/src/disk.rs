//! The disk limit of a unit (T-074, SPEC §10.4). A host with no root has
//! no sized tmpfs and no quota, so slots measures: every [`CHECK_MS`] it
//! sums the bytes under the root of each running unit (one file system, no
//! link followed), and ends a unit over its bound with an audit line
//! `limit` (`disk`) and the exit reason `limit`. The bound is the `disk` limit of the offer, else
//! `[unit.X] disk_max_bytes`; with neither, nothing is measured.

use std::os::unix::fs::MetadataExt;
use std::path::Path;

use podrunner_schema::ExitReason;
use podrunner_unit::Signal;

use crate::state::State;
use crate::{Slots, audit};

/// The time between two measures of the disk of the units.
pub const CHECK_MS: u64 = 30_000;

/// The bytes of the files under `root`, on its file system, with no link
/// followed.
pub fn bytes_under(root: &Path) -> u64 {
    let Ok(top) = root.symlink_metadata() else { return 0 };
    let dev = top.dev();
    let mut total = 0;
    let mut todo = vec![root.to_path_buf()];
    while let Some(dir) = todo.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let Ok(m) = e.path().symlink_metadata() else { continue };
            if m.dev() != dev {
                continue;
            }
            if m.is_dir() {
                todo.push(e.path());
            } else {
                total += m.len();
            }
        }
    }
    total
}

impl Slots {
    /// Measure each running unit that has a bound, when the time comes.
    pub(crate) fn check_disks(&mut self, now: u64) {
        if now < self.disk_next_ms {
            return;
        }
        self.disk_next_ms = now + CHECK_MS;
        let running: Vec<_> = self
            .jobs
            .iter()
            .filter(|(_, j)| matches!(j.state, State::Started | State::Running) && j.term_ms.is_none())
            .map(|(k, _)| k.clone())
            .collect();
        for key in running {
            let j = &self.jobs[&key];
            let (Some(unit), Some(p)) = (j.unit.clone(), j.placement.clone()) else { continue };
            let kind = &self.kinds[p.kind];
            let bound = j.offer.as_ref().and_then(|o| o.limits.get("disk").copied()).or(kind.cfg.disk_max_bytes);
            let Some(bound) = bound else { continue };
            let Some(root) = kind.driver.observe(&unit).ok().and_then(|o| o.unit_root) else { continue };
            if bytes_under(&root) > bound {
                self.audit(audit::Kind::Limit, key.0, &key.1, None, Some("disk".into()));
                self.end(&key, ExitReason::Limit, Signal::Term, now);
            }
        }
    }
}
