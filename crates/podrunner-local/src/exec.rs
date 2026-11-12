//! The executors of a local run (T-089). [`RunnerDir`] runs each job with
//! an unpacked release of the official runner on this host, one job at a
//! time, against a fake broker on the loopback: the jobs run as the user
//! of podrunner, with no unit (tier `local`, call C14).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use podrunner_broker_fake::runner::run_once;
use podrunner_broker_fake::{Broker, Job};
use podrunner_schema::JobId;

use crate::drive::{End, Executor, result_of};

/// An unpacked release of the official runner, in a directory that the run
/// may change.
pub struct RunnerDir {
    pub dir: PathBuf,
    /// The log of each job, for the caller.
    pub logs: Vec<(JobId, String)>,
}

impl RunnerDir {
    pub fn new(dir: PathBuf) -> Result<RunnerDir, String> {
        if !dir.join("run.sh").is_file() || !dir.join("bin/Runner.Listener").is_file() {
            return Err(format!("{}: no unpacked runner (run.sh, bin/Runner.Listener)", dir.display()));
        }
        Ok(RunnerDir { dir, logs: Vec::new() })
    }
}

impl Executor for RunnerDir {
    fn run(&mut self, job: &JobId, message: Job) -> Result<End, String> {
        let broker = Broker::start().map_err(|e| format!("the fake broker: {e}"))?;
        broker.queue(message);
        // Each job starts in a fresh work directory.
        let _ = std::fs::remove_dir_all(self.dir.join("_work"));
        run_once(&self.dir, &broker, &[]);
        // The runner sends `completejob` before it ends; give the broker
        // time to record it.
        let t = Instant::now();
        while broker.completions().is_empty() && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        self.logs.push((job.clone(), broker.logs()));
        let done = broker.completions();
        let Some(end) = done.first() else {
            return Err("the runner ended with no result".into());
        };
        Ok(End { result: result_of(&end.conclusion), outputs: end.outputs.clone() })
    }
}
