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

/// The time limit of a job in a unit when the job names none: the 360
/// minutes of GitHub.
const UNIT_WAIT: Duration = Duration::from_secs(360 * 60);

/// Each job in a fresh unit of a driver (T-090): the runner image, the
/// runner files of the fake broker for the job (mode 0600, no argv, no
/// environment), `run.sh` as the top process, then `destroy`. A unit with
/// residue fails its job.
pub struct InUnit {
    pub driver: std::sync::Arc<dyn podrunner_unit::UnitDriver>,
    pub image: podrunner_unit::Prepared,
    /// `run.sh` of the runner in the image.
    pub run_sh: String,
    /// How long the broker may take to record `completejob` after the
    /// end of the unit.
    pub grace: Duration,
    next: u64,
    pub logs: Vec<(JobId, String)>,
}

impl InUnit {
    pub fn new(driver: std::sync::Arc<dyn podrunner_unit::UnitDriver>, image: podrunner_unit::Prepared) -> InUnit {
        InUnit {
            driver,
            image,
            run_sh: "/home/runner/run.sh".into(),
            grace: Duration::from_secs(5),
            next: 0,
            logs: Vec::new(),
        }
    }
}

impl Executor for InUnit {
    fn run(&mut self, job: &JobId, message: Job) -> Result<End, String> {
        use podrunner_schema::{Credential, CredentialFile, Secret, UnitId};
        let broker = Broker::start().map_err(|e| format!("the fake broker: {e}"))?;
        broker.queue(message);
        self.next += 1;
        let id = UnitId::parse(&format!("u-{}", self.next)).map_err(|e| format!("unit id: {e:?}"))?;
        let runner = format!("podrunner-local-{}", self.next);
        let files = podrunner_broker_fake::files::runner_files(
            &broker.base,
            &runner,
            &podrunner_broker_fake::runner::rsaparams(),
        )
        .into_iter()
        .map(|(name, text)| CredentialFile { name, content: Secret::new(text) })
        .collect();
        let spec = podrunner_unit::StartSpec {
            unit: id.clone(),
            image: self.image.clone(),
            argv: vec![self.run_sh.clone()],
            env: Default::default(),
            credential: Credential { files },
            sockets: None,
        };
        self.driver.start(&spec).map_err(|e| format!("start {id}: {e}"))?;
        let waited = self.driver.wait(&id, UNIT_WAIT);
        let t = Instant::now();
        while broker.completions().is_empty() && t.elapsed() < self.grace {
            std::thread::sleep(Duration::from_millis(50));
        }
        let residue = self.driver.destroy(&id).map_err(|e| format!("destroy {id}: {e}"))?;
        self.logs.push((job.clone(), broker.logs()));
        let exit = waited.map_err(|e| format!("wait {id}: {e}"))?;
        if !residue.is_clean() {
            return Err(format!("unit {id} left residue: {residue:?}"));
        }
        let done = broker.completions();
        let Some(end) = done.first() else {
            return Err(format!("the runner in {id} ended with no result (exit {exit:?})"));
        };
        Ok(End { result: result_of(&end.conclusion), outputs: end.outputs.clone() })
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

#[cfg(test)]
mod tests;
