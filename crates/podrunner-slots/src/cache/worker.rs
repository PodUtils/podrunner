//! The thread of the caches (T-480): each walk and each removal of a cache
//! runs here, off the tick. A cache can hold many files; a walk in the tick
//! would hold the answers to the units, the heartbeats and the limits.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::SystemTime;

use super::{CacheConfig, Grant, Key, store};

/// A work of the thread.
pub(crate) enum Task {
    /// The bytes of the cache of a running `rw` job, up to `max`.
    Size { key: Key, data: PathBuf, max: u64 },
    /// The end of a `rw` job: its bytes, then the bound of the directory.
    End { key: Key, grant: Grant, cfg: Box<CacheConfig>, used: BTreeSet<String>, now: SystemTime },
    /// Remove a cache that the tick renamed to a trash name.
    Remove(PathBuf),
}

/// A result of the thread.
pub(crate) enum Done {
    Size {
        key: Key,
        bytes: u64,
    },
    /// The bytes that the job wrote, and each cache to remove with why.
    End {
        key: Key,
        written: u64,
        gone: Vec<(String, &'static str)>,
    },
}

/// The thread, the lock of the directory and the jobs whose result is
/// not back.
pub(crate) struct Worker {
    tx: mpsc::Sender<Task>,
    rx: mpsc::Receiver<Done>,
    pub(crate) root: PathBuf,
    pub(crate) sizing: BTreeSet<Key>,
    pub(crate) ending: BTreeSet<Key>,
    /// The time of the next measure of the caches of the running jobs.
    pub(crate) next_ms: u64,
    _lock: std::fs::File,
}

impl Worker {
    /// Lock the directory `root` and start the thread.
    pub(crate) fn start(root: PathBuf) -> std::io::Result<Worker> {
        let lock = store::lock(&root)?;
        let (tx, tasks) = mpsc::channel::<Task>();
        let (done, rx) = mpsc::channel::<Done>();
        std::thread::Builder::new().name("podrunner-cache".into()).spawn(move || {
            for task in tasks {
                if let Some(d) = run(task) {
                    let _ = done.send(d);
                }
            }
        })?;
        Ok(Worker { tx, rx, root, sizing: BTreeSet::new(), ending: BTreeSet::new(), next_ms: 0, _lock: lock })
    }

    /// Give the thread a task; false when the thread is gone.
    pub(crate) fn send(&self, task: Task) -> bool {
        self.tx.send(task).is_ok()
    }

    /// Each result that is back.
    pub(crate) fn done(&self) -> Vec<Done> {
        self.rx.try_iter().collect()
    }
}

/// One task. A panic of a walk gives a result with no bytes, so that no
/// job waits for ever.
fn run(task: Task) -> Option<Done> {
    let guarded = |f: &dyn Fn() -> Done, fallback: Done| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or(fallback)
    };
    match task {
        Task::Size { key, data, max } => Some(guarded(
            &|| Done::Size { key: key.clone(), bytes: store::size_to(&data, max) },
            Done::Size { key: key.clone(), bytes: 0 },
        )),
        Task::End { key, grant, cfg, used, now } => Some(guarded(
            &|| {
                let (bytes, written) = store::measure(&grant.data, grant.since);
                let gone = store::bound(&grant.root, &cfg, &used, &grant.id, bytes, now);
                Done::End { key: key.clone(), written, gone }
            },
            Done::End { key: key.clone(), written: 0, gone: Vec::new() },
        )),
        Task::Remove(trash) => {
            if let Err(e) = store::remove(&trash) {
                eprintln!("podrunner: {}: the removal failed: {e}", trash.display());
            }
            None
        }
    }
}
