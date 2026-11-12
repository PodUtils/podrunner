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
    /// The archives of the actions (`OWNER_REPO/SHA.tar.gz`, T-091).
    pub actions: Option<PathBuf>,
    /// The tarball of the local tree for `actions/checkout` (T-092).
    pub checkout: Option<PathBuf>,
}

impl RunnerDir {
    pub fn new(dir: PathBuf) -> Result<RunnerDir, String> {
        if !dir.join("run.sh").is_file() || !dir.join("bin/Runner.Listener").is_file() {
            return Err(format!("{}: no unpacked runner (run.sh, bin/Runner.Listener)", dir.display()));
        }
        Ok(RunnerDir { dir, logs: Vec::new(), actions: None, checkout: None })
    }
}

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
    /// The archives of the actions (`OWNER_REPO/SHA.tar.gz`, T-091).
    pub actions: Option<PathBuf>,
    /// The tarball of the local tree for `actions/checkout` (T-092).
    pub checkout: Option<PathBuf>,
    /// The egress of a held unit (T-095): the egress module and the
    /// policy of each unit (`broker-policy.v1`).
    pub egress: Option<(std::sync::Arc<podrunner_egress::Egress>, String)>,
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
            actions: None,
            checkout: None,
            egress: None,
            next: 0,
            logs: Vec::new(),
        }
    }
}

/// The container hooks of one unit (T-093): the relay and the control
/// socket in its socket directory, served by a thread until [`Hooks::stop`].
struct Hooks {
    dir: PathBuf,
    env: std::collections::BTreeMap<String, String>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    unit: std::sync::Arc<std::sync::Mutex<podrunner_hook::unit::HookUnit>>,
    thread: std::thread::JoinHandle<()>,
}

impl Hooks {
    /// End the server; the refusals of the hook calls.
    fn stop(self) -> Vec<String> {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        // The server checks its flag at each connection: wake it.
        let _ = std::os::unix::net::UnixStream::connect(self.dir.join("ctl.sock"));
        let _ = self.thread.join();
        self.unit.lock().map(|u| u.refusals.iter().map(|r| format!("{r:?}")).collect()).unwrap_or_default()
    }
}

/// The images of a job: `container:`, each service and each Docker step.
fn images(message: &Job) -> Vec<String> {
    use serde_json::Value;
    // A container token: the image itself, or the `image` of a mapping.
    fn of(v: &Value) -> Option<String> {
        match v {
            Value::String(s) => Some(s.clone()),
            Value::Object(_) => {
                v["map"].as_array()?.iter().find(|e| e["Key"] == "image")?["Value"].as_str().map(str::to_string)
            }
            _ => None,
        }
    }
    let o = &message.overrides;
    let mut all: Vec<String> = o.get("jobContainer").and_then(of).into_iter().collect();
    let services = o.get("jobServiceContainers").and_then(|s| s["map"].as_array().cloned()).unwrap_or_default();
    all.extend(services.iter().filter_map(|e| of(&e["Value"])));
    let steps = o.get("steps").and_then(|s| s.as_array().cloned()).unwrap_or_default();
    all.extend(
        steps
            .iter()
            .filter(|s| s["reference"]["type"] == "containerRegistry")
            .filter_map(|s| s["reference"]["image"].as_str().map(str::to_string)),
    );
    all
}

/// The registry of each image of the job: the first part of its name when
/// it names a host, else `docker.io`.
fn registries(message: &Job) -> Vec<String> {
    let mut hosts: Vec<String> = images(message)
        .iter()
        .map(|i| match i.split_once('/') {
            Some((h, _)) if h.contains('.') || h.contains(':') || h == "localhost" => h.to_string(),
            _ => "docker.io".to_string(),
        })
        .collect();
    hosts.sort();
    hosts.dedup();
    hosts
}

impl InUnit {
    fn hooks(&self, dir: &std::path::Path, message: &Job, id: &podrunner_schema::UnitId) -> Result<Hooks, String> {
        use podrunner_hook::policy::Policy;
        use podrunner_hook::server::{Unit, bind, serve_until};
        use std::os::unix::fs::PermissionsExt;
        let Some(guest) = self.driver.socket_mount().map(str::to_string) else {
            return Err("the job needs containers, and this unit kind has no container hooks".into());
        };
        let dir = dir.to_path_buf();
        std::fs::create_dir_all(dir.join("logs")).map_err(|e| format!("{}: {e}", dir.display()))?;
        let relay = dir.join("podrunner-hook.sh");
        std::fs::write(&relay, podrunner_hook::RELAY).map_err(|e| format!("the relay: {e}"))?;
        std::fs::set_permissions(&relay, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("the relay: {e}"))?;
        let policy = Policy {
            container: podrunner_schema::ContainerRoute::Hooks,
            shared_host_net: true,
            registries: registries(message),
            work_root: "/home/runner".into(),
        };
        let unit = std::sync::Arc::new(std::sync::Mutex::new(podrunner_hook::unit::HookUnit::new(
            self.driver.clone(),
            id.clone(),
            policy,
            None,
            dir.join("logs"),
        )));
        let listener = bind(&dir.join("ctl.sock")).map_err(|e| format!("the control socket: {e}"))?;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (u, s): (std::sync::Arc<std::sync::Mutex<dyn Unit>>, _) = (unit.clone(), stop.clone());
        let thread = std::thread::spawn(move || serve_until(listener, u, s));
        let env = std::collections::BTreeMap::from([
            ("ACTIONS_RUNNER_CONTAINER_HOOKS".to_string(), format!("{guest}/podrunner-hook.sh")),
            ("PODRUNNER_CTL".to_string(), format!("{guest}/ctl.sock")),
        ]);
        Ok(Hooks { dir, env, stop, unit, thread })
    }
}

impl Executor for InUnit {
    fn run(&mut self, job: &JobId, message: Job, limit: Duration) -> Result<End, String> {
        use podrunner_schema::{Credential, CredentialFile, Secret, UnitId};
        let broker = Broker::start().map_err(|e| format!("the fake broker: {e}"))?;
        if let Some(dir) = &self.actions {
            broker.serve_actions(dir);
        }
        if let Some(tree) = &self.checkout {
            broker.serve_checkout(tree);
        }
        let message_copy = message.clone();
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
        // One socket directory for each unit: the container hooks and the
        // egress socket of the hold (T-095).
        let containers = needs_containers(&message_copy);
        let sockets = (containers || self.egress.is_some())
            .then(|| std::env::temp_dir().join(format!("podrunner-local-unit-{}-{id}", std::process::id())));
        if let Some(dir) = &sockets {
            let _ = std::fs::remove_dir_all(dir);
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let end_sockets = |sockets: &Option<PathBuf>| {
            if let Some((egress, _)) = &self.egress {
                egress.close(id.as_str());
            }
            if let Some(dir) = sockets {
                let _ = std::fs::remove_dir_all(dir);
            }
        };
        if let (Some((egress, policy)), Some(dir)) = (&self.egress, &sockets) {
            let socket = dir.join(podrunner_unit::EGRESS_SOCKET);
            if let Err(e) = egress.open(id.as_str(), policy, &socket, limit) {
                end_sockets(&sockets);
                return Err(format!("the egress of {id}: {e}"));
            }
        }
        let hooks = match (containers, &sockets) {
            (true, Some(dir)) => match self.hooks(dir, &message_copy, &id) {
                Ok(h) => Some(h),
                Err(e) => {
                    end_sockets(&sockets);
                    return Err(e);
                }
            },
            _ => None,
        };
        let spec = podrunner_unit::StartSpec {
            unit: id.clone(),
            image: self.image.clone(),
            argv: vec![self.run_sh.clone()],
            env: hooks.as_ref().map(|h| h.env.clone()).unwrap_or_default(),
            credential: Credential { files },
            sockets: sockets.clone(),
        };
        let started = self.driver.start(&spec).map_err(|e| format!("start {id}: {e}"));
        if let Err(e) = started {
            if let Some(h) = hooks {
                h.stop();
            }
            end_sockets(&sockets);
            return Err(e);
        }
        let waited = self.driver.wait(&id, limit);
        // Still running at the limit: end the unit (destroy below).
        let timed_out = matches!(waited, Ok(None));
        let t = Instant::now();
        while broker.completions().is_empty() && t.elapsed() < self.grace {
            std::thread::sleep(Duration::from_millis(50));
        }
        let refusals = hooks.map(Hooks::stop).unwrap_or_default();
        let residue = self.driver.destroy(&id).map_err(|e| format!("destroy {id}: {e}"));
        end_sockets(&sockets);
        let residue = residue?;
        let mut log = broker.logs();
        for r in &refusals {
            log.push_str(&format!("\npodrunner: a hook call was refused: {r}"));
        }
        self.logs.push((job.clone(), log));
        let exit = waited.map_err(|e| format!("wait {id}: {e}"))?;
        if timed_out {
            return Err(format!("the job ran past its timeout-minutes ({} s); unit {id} was ended", limit.as_secs()));
        }
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

/// The job needs the container hooks of a unit: `container:`, `services:`
/// or a Docker action (T-093).
pub fn needs_containers(message: &Job) -> bool {
    let o = &message.overrides;
    let docker = o
        .get("steps")
        .and_then(|s| s.as_array())
        .is_some_and(|s| s.iter().any(|s| s["reference"]["type"] == "containerRegistry"));
    o.contains_key("jobContainer") || o.contains_key("jobServiceContainers") || docker
}

impl Executor for RunnerDir {
    fn run(&mut self, job: &JobId, message: Job, _limit: Duration) -> Result<End, String> {
        if needs_containers(&message) {
            return Err("the job needs containers: run it in a unit (--podvm), not on a runner directory".into());
        }
        let broker = Broker::start().map_err(|e| format!("the fake broker: {e}"))?;
        if let Some(dir) = &self.actions {
            broker.serve_actions(dir);
        }
        if let Some(tree) = &self.checkout {
            broker.serve_checkout(tree);
        }
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
