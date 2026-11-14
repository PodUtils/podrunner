//! The executors of a local run (T-089). [`RunnerDir`] runs each job with
//! an unpacked release of the official runner on this host, one job at a
//! time, against a fake broker on the loopback: the jobs run as the user
//! of podrunner, with no unit (tier `local`, call C14). [`InUnit`] runs
//! each job in a fresh unit of a driver (T-090).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pod_actions_workflow::JobResult;
use podrunner_broker_fake::artifacts::ArtifactStore;
use podrunner_broker_fake::cache::CacheStore;
use podrunner_broker_fake::{Broker, Job};
use podrunner_schema::JobId;

use crate::drive::{End, Executor, result_of};

mod host;
mod images;
mod toolcache;

pub use host::RunnerDir;
#[cfg(test)]
use images::images;
use images::registries;
pub use toolcache::{TOOLCACHE_MOUNT, toolcache_argv};

/// The count of the units of the process.
static NEXT_UNIT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
    /// The commit of each `OWNER/REPO@REF` of the run (T-118).
    pub pins: BTreeMap<String, String>,
    /// The artifact store of the run (T-119).
    pub artifacts: Option<Arc<ArtifactStore>>,
    /// The cache store (T-120).
    pub cache: Option<Arc<CacheStore>>,
    /// The issuer of the id-tokens of the run (T-140).
    pub oidc: Option<Arc<podrunner_broker_fake::oidc::Issuer>>,
    /// The tarball of the local tree for `actions/checkout` (T-092).
    pub checkout: Option<PathBuf>,
    /// The egress of a held unit (T-095): the egress module and the
    /// policy of each unit (`broker-policy.v1`).
    pub egress: Option<(std::sync::Arc<podrunner_egress::Egress>, String)>,
    /// The egress policy of each environment, and the environment of the
    /// next job (T-136).
    pub environment_policies: BTreeMap<String, String>,
    next_environment: Option<String>,
    /// The live sink and the blobs of each job (T-134).
    pub live: Option<Live>,
    pub blobs: Vec<(JobId, Vec<(String, String)>)>,
    /// A guest unit (the microvm or the incus kind, T-123): no NIC, and
    /// [`crate::front`] on its egress socket; no container hooks (a guest
    /// runs its containers in its own engine).
    pub guest: bool,
    /// The registry of the pinned images for the engine of a guest (T-157).
    pub mirror: Option<Arc<podrunner_broker_fake::mirror::Mirror>>,
    /// The stop flag of the next job (T-132).
    stop: Option<crate::cancel::Stop>,
    /// `--export DIR`: the work directory of the runner is a host directory,
    /// and its workspaces go to `DIR/JOB` after the job (T-138).
    pub export: Option<PathBuf>,
    /// `--toolcache DIR`: DIR, read-only in the unit; the tool cache of the
    /// runner links to its tools at the start (T-138).
    pub toolcache: Option<PathBuf>,
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
            pins: BTreeMap::new(),
            artifacts: None,
            cache: None,
            oidc: None,
            checkout: None,
            egress: None,
            environment_policies: BTreeMap::new(),
            next_environment: None,
            live: None,
            blobs: Vec::new(),
            guest: false,
            mirror: None,
            stop: None,
            export: None,
            toolcache: None,
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
        self.unit
            .lock()
            .map(|u| {
                let codes = u.refusals.iter().map(|r| format!("{r:?}"));
                // The cause of each refusal of the driver, for the log of the run.
                codes.chain(u.driver_errors.iter().map(|e| format!("the driver: {e}"))).collect()
            })
            .unwrap_or_default()
    }
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

/// The prefix of a line of a job log for a request with no route (T-145).
pub const NO_ROUTE: &str = "podrunner: the fake broker has no route for ";

/// A sink of the live lines of a run: the job and a part of its log
/// (T-134).
pub type Live = Arc<dyn Fn(&JobId, &str) + Send + Sync>;

/// Give the new text of each log blob to the live sink.
fn pump(live: &Option<Live>, job: &JobId, broker: &Broker) {
    if let Some(l) = live {
        for (_, text) in broker.take_new() {
            l(job, &text);
        }
    }
}

/// The end of a job: the last live lines, the blobs of the job (its step
/// logs and summaries, T-134), and its log.
fn record(
    live: &Option<Live>,
    blobs: &mut Vec<(JobId, Vec<(String, String)>)>,
    job: &JobId,
    broker: &Broker,
) -> String {
    pump(live, job, broker);
    blobs.push((job.clone(), broker.blobs()));
    job_log(broker)
}

/// The log of a job: the logs that the runner uploaded, and one line for
/// each request that the broker had no route for (T-145).
fn job_log(broker: &Broker) -> String {
    let mut log = broker.logs();
    for m in broker.missing() {
        log.push_str(&format!("\n{NO_ROUTE}{m}"));
    }
    log
}

impl Executor for InUnit {
    fn take_logs(&mut self) -> Vec<(JobId, String)> {
        std::mem::take(&mut self.logs)
    }

    fn set_live(&mut self, live: Live) {
        self.live = Some(live);
    }

    fn set_stop(&mut self, stop: crate::cancel::Stop) {
        self.stop = Some(stop);
    }

    fn environment(&mut self, name: Option<&str>) {
        self.next_environment = name.map(str::to_string);
    }

    fn take_blobs(&mut self) -> Vec<(JobId, Vec<(String, String)>)> {
        std::mem::take(&mut self.blobs)
    }

    fn run(&mut self, job: &JobId, message: Job, limit: Duration) -> Result<End, String> {
        use podrunner_schema::{Credential, CredentialFile, Secret, UnitId};
        let broker = if self.guest { Broker::start_seen_at(crate::front::BROKER) } else { Broker::start() };
        let broker = broker.map_err(|e| format!("the fake broker: {e}"))?;
        if let Some(dir) = &self.actions {
            broker.serve_actions(dir, self.pins.clone());
        }
        if let Some(store) = &self.artifacts {
            broker.serve_artifacts(store.clone());
        }
        if let Some(store) = &self.cache {
            broker.serve_cache(store.clone());
        }
        if let Some(issuer) = &self.oidc {
            broker.serve_oidc(issuer.clone());
        }
        if let Some(tree) = &self.checkout {
            broker.serve_checkout(tree);
        }
        if let Some(m) = &self.mirror {
            broker.serve_mirror(m.clone());
        }
        let message_copy = message.clone();
        broker.queue(message);
        // One count for the process: the units of parallel jobs (T-132)
        // never share an id, a socket directory or an egress grant.
        let n = NEXT_UNIT.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let id = UnitId::parse(&format!("u-{n}")).map_err(|e| format!("unit id: {e:?}"))?;
        let runner = format!("podrunner-local-{n}");
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
        let containers = needs_containers(&message_copy) && !self.guest;
        let sockets = (containers || self.egress.is_some() || self.guest)
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
        // A guest: the egress module on an inner socket, behind the front.
        let egress_socket = |dir: &PathBuf| match self.guest {
            true => dir.join("egress-inner.sock"),
            false => dir.join(podrunner_unit::EGRESS_SOCKET),
        };
        // The policy of the environment of the job, else that of the run (T-136).
        let env_policy = self.next_environment.as_ref().and_then(|n| self.environment_policies.get(n));
        if let (Some((egress, policy)), Some(dir)) = (&self.egress, &sockets)
            && let Err(e) = egress.open(id.as_str(), env_policy.unwrap_or(policy), &egress_socket(dir), limit)
        {
            end_sockets(&sockets);
            return Err(format!("the egress of {id}: {e}"));
        }
        let _front = match (&sockets, self.guest) {
            (Some(dir), true) => {
                let port: u16 =
                    broker.base.trim_end_matches('/').rsplit(':').next().and_then(|p| p.parse().ok()).unwrap_or(0);
                let inner = self.egress.as_ref().map(|_| egress_socket(dir));
                match crate::front::serve(&dir.join(podrunner_unit::EGRESS_SOCKET), port, inner) {
                    Ok(f) => Some(f),
                    Err(e) => {
                        end_sockets(&sockets);
                        return Err(format!("the front of {id}: {e}"));
                    }
                }
            }
            _ => None,
        };
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
        // The work directory on the host, open to the runner user of the unit.
        let work = self
            .export
            .as_ref()
            .map(|_| std::env::temp_dir().join(format!("podrunner-run-export-{}-{id}", std::process::id())));
        if let Some(w) = &work {
            let made = std::fs::create_dir_all(w)
                .and_then(|()| std::fs::set_permissions(w, std::os::unix::fs::PermissionsExt::from_mode(0o777)));
            if let Err(e) = made {
                end_sockets(&sockets);
                return Err(format!("the work directory {}: {e}", w.display()));
            }
        }
        let run_root = self.run_sh.rsplit_once('/').map_or("/home/runner", |x| x.0).to_string();
        let mut mounts: Vec<_> = work.iter().map(|w| (w.clone(), format!("{run_root}/_work"))).collect();
        let argv = match &self.toolcache {
            Some(dir) => {
                mounts.push((dir.clone(), format!("{TOOLCACHE_MOUNT}:ro")));
                toolcache_argv(&self.run_sh, &run_root)
            }
            None => vec![self.run_sh.clone()],
        };
        let spec = podrunner_unit::StartSpec {
            unit: id.clone(),
            image: self.image.clone(),
            argv,
            env: hooks.as_ref().map(|h| h.env.clone()).unwrap_or_default(),
            credential: Credential { files },
            sockets: sockets.clone(),
            mounts,
        };
        let started = self.driver.start(&spec).map_err(|e| format!("start {id}: {e}"));
        if let Err(e) = started {
            if let Some(h) = hooks {
                h.stop();
            }
            end_sockets(&sockets);
            if let Some(w) = &work {
                let _ = crate::work::clear(w);
            }
            return Err(e);
        }
        // Wait in slices: a signal destroys the unit at once (T-141).
        let until = Instant::now() + limit;
        let waited = loop {
            let slice = until.saturating_duration_since(Instant::now()).min(Duration::from_secs(1));
            pump(&self.live, job, &broker);
            match self.driver.wait(&id, slice) {
                Ok(None) if !crate::cancel::stopped(&self.stop) && Instant::now() < until => continue,
                other => break other,
            }
        };
        let cancelled = crate::cancel::stopped(&self.stop) && matches!(waited, Ok(None));
        // Still running at the limit: end the unit (destroy below).
        let timed_out = matches!(waited, Ok(None));
        let t = Instant::now();
        while broker.completions().is_empty() && t.elapsed() < self.grace {
            std::thread::sleep(Duration::from_millis(50));
        }
        let refusals = hooks.map(Hooks::stop).unwrap_or_default();
        // No result: keep the end of the output of the unit, before it goes.
        let output = match broker.completions().is_empty() {
            true => self.driver.unit_logs(&id).map(|o| tail(&o)).ok(),
            false => None,
        };
        // The hosts that the egress policy of the unit refused, before the
        // grant ends: the log names them.
        let denied = self.egress.as_ref().map(|(e, _)| e.denials(id.as_str())).unwrap_or_default();
        let residue = self.driver.destroy(&id).map_err(|e| format!("destroy {id}: {e}"));
        end_sockets(&sockets);
        // The workspaces of the job, also of a failed one (T-138).
        let exported = match (&work, &self.export) {
            (Some(w), Some(dir)) => {
                let r = crate::work::export(w, &dir.join(job.as_str()));
                let _ = crate::work::clear(w);
                r.err()
            }
            _ => None,
        };
        let residue = residue?;
        let mut log = record(&self.live, &mut self.blobs, job, &broker);
        for r in &refusals {
            log.push_str(&format!("\npodrunner: a hook call was refused: {r}"));
        }
        if let Some(o) = output {
            log.push_str(&format!("\npodrunner: the end of the output of unit {id}:\n{o}"));
        }
        if let Some(e) = exported {
            log.push_str(&format!("\npodrunner: {e}"));
        }
        for d in &denied {
            log.push_str(&format!("\npodrunner: the egress policy of {id} denied {d}"));
        }
        // A job that did not succeed: the last requests of its broker.
        if !broker.completions().iter().any(|c| c.conclusion == "succeeded") {
            let seen = broker.seen();
            let last = &seen[seen.len().saturating_sub(15)..];
            log.push_str(&format!("\npodrunner: the last requests of the broker: {}", last.join(", ")));
        }
        self.logs.push((job.clone(), log));
        let exit = waited.map_err(|e| format!("wait {id}: {e}"))?;
        if cancelled {
            return Ok(End { result: JobResult::Cancelled, outputs: vec![] });
        }
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

/// The last 8 KiB of an output, as text.
fn tail(out: &[u8]) -> String {
    let start = out.len().saturating_sub(8 * 1024);
    String::from_utf8_lossy(&out[start..]).into_owned()
}

#[cfg(test)]
mod tests;
