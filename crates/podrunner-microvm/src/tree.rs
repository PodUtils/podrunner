//! The processes and the run directories that a guest has on the host.

use std::fs;
use std::path::{Path, PathBuf};

use podrunner_unit::conformance::alive;
use podrunner_unit::{procs, shim};

use crate::Running;

impl Running {
    /// The live processes of the unit: each with its tag and, while the
    /// `podvm run` child is not reaped, the child and its tree. A process
    /// in its `exec` shows an empty environment, so the tag misses it
    /// (T-730). An unreaped child keeps its pid, and a signal that waits
    /// on a process stays across its `exec`.
    pub(crate) fn pids(&self) -> Vec<u32> {
        let mut pids = procs::tagged(&self.tag);
        if self.exit.is_none() {
            let top = self.child.id();
            pids.extend(shim::descendants(top));
            pids.extend(Some(top).filter(|p| alive(*p)));
        }
        pids.sort_unstable();
        pids.dedup();
        pids
    }
}

/// The run directories that the podvm process `pid` made in the store
/// `images` (`machine/run-PID-*`).
pub(crate) fn runs_of(images: &Path, pid: u32) -> Vec<PathBuf> {
    let prefix = format!("run-{pid}-");
    let Ok(entries) = fs::read_dir(images.join("machine")) else { return Vec::new() };
    entries.flatten().filter(|e| e.file_name().to_string_lossy().starts_with(&prefix)).map(|e| e.path()).collect()
}

/// Remove the run directories of [`runs_of`].
pub(crate) fn remove_runs(runs: &[PathBuf]) {
    for r in runs {
        let _ = fs::remove_dir_all(r);
    }
}
