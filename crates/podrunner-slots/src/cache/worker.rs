//! The threads of the caches (T-480): each walk of a cache runs on one
//! thread, and each removal on a second one, off the tick. A cache can hold
//! many files; a walk in the tick would hold the answers to the units, the
//! heartbeats and the limits, and a removal would hold the count of the
//! end of a job. A panic of a thread ends `serve` in a release build
//! (`panic = "abort"`); in a debug build the thread ends, and each report
//! that waits for it goes out after `[deadlines] report_s`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::SystemTime;

use super::{CacheConfig, Grant, Key, Uses, store};

/// A work of the thread of the walks.
pub(crate) enum Task {
    /// The bytes of the cache of a running `rw` job, up to `max`.
    Size { key: Key, data: PathBuf, max: u64 },
    /// The end of a `rw` job: its bytes, then the bound of the directory.
    /// `stamp`: the opens of its scope when the tick sent it.
    End { key: Key, grant: Grant, cfg: Box<CacheConfig>, stamp: u64, now: SystemTime },
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

/// The threads, the lock of the directory, the scopes in use and the jobs
/// whose result is not back.
pub(crate) struct Worker {
    tx: mpsc::Sender<Task>,
    rx: mpsc::Receiver<Done>,
    trash: mpsc::Sender<PathBuf>,
    pub(crate) root: PathBuf,
    pub(crate) uses: Arc<Mutex<Uses>>,
    pub(crate) sizing: BTreeSet<Key>,
    /// The scope of each `rw` job whose count is not back: the scope stays
    /// in use, also after the report of the job.
    pub(crate) ending: BTreeMap<Key, String>,
    /// The time of the next measure of the caches of the running jobs.
    pub(crate) next_ms: u64,
    /// A test holds it to hold the thread of the walks.
    #[cfg(test)]
    pub(crate) gate: Arc<Mutex<()>>,
    _lock: std::fs::File,
}

impl Worker {
    /// Lock the directory `root`, start the threads, and remove each trash
    /// that a stop left.
    pub(crate) fn start(root: PathBuf) -> std::io::Result<Worker> {
        let lock = store::lock(&root)?;
        let uses = Arc::new(Mutex::new(Uses::default()));
        let (tx, tasks) = mpsc::channel::<Task>();
        let (done, rx) = mpsc::channel::<Done>();
        let shared = uses.clone();
        let gate = Arc::new(Mutex::new(()));
        let held = gate.clone();
        std::thread::Builder::new().name("podrunner-cache".into()).spawn(move || {
            for task in tasks {
                let _held = held.lock();
                if done.send(run(task, &shared)).is_err() {
                    return;
                }
            }
        })?;
        let (trash, removals) = mpsc::channel::<PathBuf>();
        std::thread::Builder::new().name("podrunner-cache-rm".into()).spawn(move || {
            for dir in removals {
                if let Err(e) = store::remove(&dir) {
                    eprintln!("podrunner: {}: the removal failed: {e}", dir.display());
                }
            }
        })?;
        for left in store::trash(&root) {
            let _ = trash.send(left);
        }
        Ok(Worker {
            tx,
            rx,
            trash,
            root,
            uses,
            sizing: BTreeSet::new(),
            ending: BTreeMap::new(),
            next_ms: 0,
            #[cfg(test)]
            gate,
            _lock: lock,
        })
    }

    /// Give the thread of the walks a task; false when it is gone.
    pub(crate) fn send(&self, task: Task) -> bool {
        self.tx.send(task).is_ok()
    }

    /// Give the thread of the removals a cache that the tick renamed.
    pub(crate) fn remove(&self, trash: PathBuf) {
        let _ = self.trash.send(trash);
    }

    /// Each result that is back.
    pub(crate) fn done(&self) -> Vec<Done> {
        self.rx.try_iter().collect()
    }

    /// The scopes in use, under the lock.
    pub(crate) fn uses(&self) -> std::sync::MutexGuard<'_, Uses> {
        self.uses.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// One task of the thread of the walks.
fn run(task: Task, uses: &Mutex<Uses>) -> Done {
    match task {
        Task::Size { key, data, max } => Done::Size { key, bytes: store::size_to(&data, max) },
        Task::End { key, grant, cfg, stamp, now } => {
            let (bytes, written) = store::measure(&grant.data, grant.since);
            let gone = store::bound(&grant.root, &cfg, uses, &grant.id, stamp, bytes, now);
            Done::End { key, written, gone }
        }
    }
}
