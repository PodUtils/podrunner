//! The executor of `--runner-dir` (T-089): the runner of an unpacked
//! release on the host, one job at a time, against a fake broker.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pod_actions_workflow::JobResult;
use podrunner_broker_fake::artifacts::ArtifactStore;
use podrunner_broker_fake::cache::CacheStore;
use podrunner_broker_fake::runner::start_runner;
use podrunner_broker_fake::{Broker, Job};
use podrunner_schema::JobId;

use super::{Live, needs_containers, pump, record};
use crate::drive::{End, Executor, result_of};

/// An unpacked release of the official runner, in a directory that the run
/// may change.
pub struct RunnerDir {
    pub dir: PathBuf,
    /// The log of each job, for the caller.
    pub logs: Vec<(JobId, String)>,
    /// The archives of the actions (`OWNER_REPO/SHA.tar.gz`, T-091).
    pub actions: Option<PathBuf>,
    /// The commit of each `OWNER/REPO@REF` of the run (T-118).
    pub pins: BTreeMap<String, String>,
    /// The artifact store of the run (T-119).
    pub artifacts: Option<Arc<ArtifactStore>>,
    /// The cache store (T-120).
    pub cache: Option<Arc<CacheStore>>,
    /// The live sink and the blobs of each job (T-134).
    pub live: Option<Live>,
    pub blobs: Vec<(JobId, Vec<(String, String)>)>,
    /// The tarball of the local tree for `actions/checkout` (T-092).
    pub checkout: Option<PathBuf>,
}

impl RunnerDir {
    pub fn new(dir: PathBuf) -> Result<RunnerDir, String> {
        if !dir.join("run.sh").is_file() || !dir.join("bin/Runner.Listener").is_file() {
            return Err(format!("{}: no unpacked runner (run.sh, bin/Runner.Listener)", dir.display()));
        }
        Ok(RunnerDir {
            dir,
            logs: Vec::new(),
            actions: None,
            pins: BTreeMap::new(),
            artifacts: None,
            cache: None,
            live: None,
            blobs: Vec::new(),
            checkout: None,
        })
    }
}

impl Executor for RunnerDir {
    fn take_logs(&mut self) -> Vec<(JobId, String)> {
        std::mem::take(&mut self.logs)
    }

    fn set_live(&mut self, live: Live) {
        self.live = Some(live);
    }

    fn take_blobs(&mut self) -> Vec<(JobId, Vec<(String, String)>)> {
        std::mem::take(&mut self.blobs)
    }

    fn run(&mut self, job: &JobId, message: Job, limit: Duration) -> Result<End, String> {
        if needs_containers(&message) {
            return Err("the job needs containers: run it in a unit (--podvm, or --microvm with --engine), not on a runner directory".into());
        }
        let broker = Broker::start().map_err(|e| format!("the fake broker: {e}"))?;
        if let Some(dir) = &self.actions {
            broker.serve_actions(dir, self.pins.clone());
        }
        if let Some(store) = &self.artifacts {
            broker.serve_artifacts(store.clone());
        }
        if let Some(store) = &self.cache {
            broker.serve_cache(store.clone());
        }
        if let Some(tree) = &self.checkout {
            broker.serve_checkout(tree);
        }
        broker.queue(message);
        // Each job starts in a fresh work directory (T-146).
        crate::work::clear(&self.dir.join("_work"))?;
        let mut child = start_runner(&self.dir, &broker, &[])?;
        // The limit of the job (`timeout-minutes`, T-115): at its end, the
        // whole process group of the runner ends, as the unit of `InUnit`.
        let end = Instant::now() + limit;
        let status = loop {
            if let Some(s) = child.try_wait().map_err(|e| format!("the runner: {}", e.kind()))? {
                break Some(s);
            }
            // A signal ends the job as its limit does (T-141).
            if Instant::now() >= end || crate::cancel::requested() {
                break None;
            }
            pump(&self.live, job, &broker);
            std::thread::sleep(Duration::from_millis(100));
        };
        let Some(status) = status else {
            end_group(child.id());
            let _ = child.wait();
            {
                let log = record(&self.live, &mut self.blobs, job, &broker);
                self.logs.push((job.clone(), log));
            }
            if crate::cancel::requested() {
                return Ok(End { result: JobResult::Cancelled, outputs: vec![] });
            }
            return Err(format!("the job ran past its limit of {} s", limit.as_secs()));
        };
        if let Some(why) = signal_reason(&status) {
            {
                let log = record(&self.live, &mut self.blobs, job, &broker);
                self.logs.push((job.clone(), log));
            }
            return Err(why);
        }
        // The runner sends `completejob` before it ends; give the broker
        // time to record it.
        let t = Instant::now();
        while broker.completions().is_empty() && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        {
            let log = record(&self.live, &mut self.blobs, job, &broker);
            self.logs.push((job.clone(), log));
        }
        let done = broker.completions();
        let Some(end) = done.first() else {
            return Err("the runner ended with no result".into());
        };
        Ok(End { result: result_of(&end.conclusion), outputs: end.outputs.clone() })
    }
}

/// End the process group of `leader`: TERM, then KILL after 5 s.
fn end_group(leader: u32) {
    let Ok(pgid) = i32::try_from(leader) else { return };
    // SAFETY: kill of a process group that this process made.
    unsafe { libc::kill(-pgid, libc::SIGTERM) };
    let t = Instant::now();
    // SAFETY: a signal 0 checks that the group still has a process.
    while t.elapsed() < Duration::from_secs(5) && unsafe { libc::kill(-pgid, 0) } == 0 {
        std::thread::sleep(Duration::from_millis(100));
    }
    // SAFETY: as above.
    unsafe { libc::kill(-pgid, libc::SIGKILL) };
}

/// The reason of a runner that a signal ended, with a word on SIGXFSZ (a
/// limit of file size, T-144).
fn signal_reason(status: &std::process::ExitStatus) -> Option<String> {
    use std::os::unix::process::ExitStatusExt;
    match status.signal()? {
        libc::SIGXFSZ => Some("the runner ended by SIGXFSZ: a limit of file size (ulimit -f) is too small".into()),
        s => Some(format!("the runner ended by signal {s}")),
    }
}
