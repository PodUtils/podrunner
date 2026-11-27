//! The executors of a local run (T-089). [`RunnerDir`] runs each job with
//! an unpacked release of the official runner on this host, one job at a
//! time, against a fake broker on the loopback: the jobs run as the user
//! of podrunner, with no unit (tier `local`, call C14). [`InUnit`] runs
//! each job in a fresh unit of a driver (T-090).

use std::collections::BTreeMap;
use std::path::PathBuf;

pub use errors::{BIND_FIX, broker_error};
use errors::{Gone, start_error};
pub use log::ADDED;
use log::{added, pump, record, tail};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pod_actions_workflow::JobResult;
use podrunner_broker_fake::artifacts::ArtifactStore;
use podrunner_broker_fake::cache::CacheStore;
use podrunner_broker_fake::{Broker, Job};
use podrunner_schema::JobId;

use crate::drive::{End, Executor, result_of};

pub mod docker;
mod errors;
mod hooks;
mod host;
mod images;
mod log;
mod noproxy;
mod proxy;
mod stopping;
pub use stopping::{going, remove_when_gone};
mod toolcache;
pub mod vnet;
pub mod workdir;

pub use host::{PASSED, RunnerDir, space_refusal};
#[cfg(test)]
use images::{images, registries};
pub use noproxy::dropped as dropped_no_proxy;
pub use proxy::node_ca;
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
    /// How long the runner of a unit may take to send its first request to
    /// the broker (T-561): past it the runner cannot reach the broker, and
    /// the job ends with that cause, not at its `timeout-minutes`.
    pub reach: Duration,
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
    /// The chosen guest route supplies an engine for its runner.
    pub guest_engine: bool,
    /// The registry of the pinned images for the engine of a guest (T-157).
    pub mirror: Option<Arc<podrunner_broker_fake::mirror::Mirror>>,
    /// The stop flag of the next job (T-132).
    stop: Option<crate::cancel::Stop>,
    /// The bound of the wait for the end of a unit at a cancel (T-716,
    /// T-720): [`crate::cancel::unit_bound`].
    pub bound: Duration,
    /// `--export DIR`: the work directory of the runner is a host directory,
    /// and its workspaces go to `DIR/JOB` after the job (T-138).
    pub export: Option<PathBuf>,
    /// `--toolcache DIR`: DIR, read-only in the unit; the tool cache of the
    /// runner links to its tools at the start (T-138).
    pub toolcache: Option<PathBuf>,
    pub logs: Vec<(JobId, String)>,
    /// Where the unit sees its credential files: a unit with the network of
    /// the host gets the proxy and the CA files of the caller there (T-284).
    pub credential_dir: Option<String>,
    /// The vnet shim of podnet for a podvm unit (T-220): the runner
    /// reaches the broker and the egress module through the front of its
    /// socket directory, and the host binds no AF_INET socket.
    pub vnet: Option<PathBuf>,
}

impl InUnit {
    pub fn new(driver: std::sync::Arc<dyn podrunner_unit::UnitDriver>, image: podrunner_unit::Prepared) -> InUnit {
        InUnit {
            driver,
            image,
            run_sh: "/home/runner/run.sh".into(),
            grace: Duration::from_secs(5),
            reach: Duration::from_secs(600),
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
            guest_engine: false,
            mirror: None,
            stop: None,
            bound: crate::cancel::unit_bound(),
            export: None,
            toolcache: None,
            logs: Vec::new(),
            credential_dir: None,
            vnet: None,
        }
    }
}

/// The prefix of a line of a job log for a request with no route (T-145).
pub const NO_ROUTE: &str = "podrunner: the fake broker has no route for ";

/// A sink of the live lines of a run: the job and a part of its log
/// (T-134).
pub type Live = Arc<dyn Fn(&JobId, &str) + Send + Sync>;

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
        // One count for the process: the units of parallel jobs (T-132)
        // never share an id, a socket directory or an egress grant.
        if self.guest && !self.guest_engine && needs_containers(&message) {
            return Err(
                "the selected guest image needs the bundled Docker engine or --engine DIR for container work".into()
            );
        }
        let src = docker::Sources { store: self.actions.as_deref(), pins: &self.pins, tree: self.checkout.as_deref() };
        docker::build_refusal(&message, &src).filter(|_| !self.guest).map_or(Ok(()), Err)?;
        let n = NEXT_UNIT.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        // A guest reaches the broker through its front: a unix socket, so
        // the host needs no AF_INET bind (T-220).
        let broker_sock = crate::runkey::in_tmp(crate::runkey::BROKER, &format!("{n}.sock"));
        let fronted = self.guest || self.vnet.is_some();
        let _gone = Gone(fronted.then(|| broker_sock.clone()));
        let broker = match fronted {
            true => Broker::start_unix(&broker_sock, crate::front::BROKER)
                .map_err(|e| format!("the fake broker: {}: {e}", broker_sock.display()))?,
            false => Broker::start().map_err(broker_error)?,
        };
        // The pins also name the trees of the checkouts in actions (T-713).
        broker.serve_actions(self.actions.as_deref(), self.pins.clone());
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
        let id = UnitId::parse(&format!("u-{n}")).map_err(|e| format!("unit id: {e:?}"))?;
        let runner = format!("podrunner-local-{n}");
        let mut files: Vec<CredentialFile> = broker
            .runner_files(&runner)
            .into_iter()
            .map(|(name, text)| CredentialFile { name, content: Secret::new(text) })
            .collect();
        workdir::set(&mut files, &workdir::work_folder(&self.run_sh))?;
        let (mut proxy_env, mut proxy_ca) = (Vec::new(), None);
        if let (false, Some(dir)) = (fronted, &self.credential_dir) {
            let (env, more, ca) = proxy::unit_env(dir, std::env::vars(), self.egress.is_some())?;
            (proxy_env, proxy_ca) = (env, ca);
            files.extend(more);
        }
        // One socket directory for each unit: the container hooks and the
        // egress socket of the hold (T-095).
        let containers = needs_containers(&message_copy) && !self.guest;
        let sockets = (containers || self.egress.is_some() || fronted)
            .then(|| crate::runkey::in_tmp(crate::runkey::UNIT_DIR, id.as_str()));
        if let Some(dir) = &sockets {
            crate::work::private_dir(dir)?;
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
        let egress_socket = |dir: &PathBuf| match fronted {
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
        // The vnet shim reaches the unit in its socket directory, mounted
        // at its host path too: on podvm's userland rung the loader of the
        // image runs on the host and opens LD_PRELOAD before a path of the
        // unit exists, so only a path that is the same on the host and in
        // the unit holds on each rung (T-561).
        let vnet_mount = match (&self.vnet, &sockets) {
            (Some(lib), Some(dir)) => {
                let placed = self
                    .driver
                    .socket_mount()
                    .ok_or_else(|| {
                        "--vnet needs the socket volume of podvm (its supervise, userland or namespace rung; `podrunner doctor --run` names the rung)"
                            .to_string()
                    })
                    .and_then(|_| vnet::host_dir(dir));
                match placed.and_then(|m| vnet::place(lib, dir).map(|()| m)) {
                    Ok(m) => Some(m),
                    Err(e) => {
                        end_sockets(&sockets);
                        return Err(e);
                    }
                }
            }
            _ => None,
        };
        let _front = match (&sockets, fronted) {
            (Some(dir), true) => {
                let inner = self.egress.as_ref().map(|_| egress_socket(dir));
                let up = crate::front::Upstream::Unix(broker_sock.clone());
                match crate::front::serve(&dir.join(podrunner_unit::EGRESS_SOCKET), up, inner) {
                    Ok(f) => Some(f),
                    Err(e) => {
                        end_sockets(&sockets);
                        // A socket path past `sun_path` (107 bytes) comes from TMPDIR.
                        let long = dir.join(podrunner_unit::EGRESS_SOCKET).as_os_str().len() > 107;
                        let fix = if long { "; set TMPDIR to a shorter path" } else { "" };
                        return Err(format!("the front of {id}: {e}{fix}"));
                    }
                }
            }
            _ => None,
        };
        let hooks = match (containers, &sockets) {
            (true, Some(dir)) => match self.hooks(dir, &message_copy, &id, proxy_ca.as_deref()) {
                Ok(h) => Some(h),
                Err(e) => {
                    end_sockets(&sockets);
                    return Err(e);
                }
            },
            _ => None,
        };
        // The work directory on the host, open to the runner user of the unit.
        let work = self.export.as_ref().map(|_| crate::runkey::in_tmp(crate::runkey::EXPORT, id.as_str()));
        // 0777 for the runner user of the unit, in a parent at 0700 (T-408).
        let work = match work.as_deref().map(crate::work::export_dir).transpose() {
            Ok(w) => w,
            Err(e) => {
                end_sockets(&sockets);
                return Err(format!("the work directory: {e}"));
            }
        };
        let folder = workdir::work_folder(&self.run_sh);
        let mut mounts: Vec<_> = work.iter().map(|w| (w.clone(), folder.clone())).collect();
        if let (Some(m), Some(dir)) = (&vnet_mount, &sockets) {
            mounts.push((dir.clone(), m.clone()));
        }
        let argv = match &self.toolcache {
            Some(dir) => {
                mounts.push((dir.clone(), format!("{TOOLCACHE_MOUNT}:ro")));
                toolcache_argv(&self.run_sh, &folder)
            }
            None => vec![self.run_sh.clone()],
        };
        let mut env = hooks.as_ref().map(|h| h.env.clone()).unwrap_or_default();
        proxy::unit_extra(&mut env, self.guest, self.egress.is_some().then_some(broker.base.as_str()));
        if let Some(m) = &vnet_mount {
            vnet::env(&mut env, m);
        }
        env.extend(proxy_env);
        let spec = podrunner_unit::StartSpec {
            unit: id.clone(),
            image: self.image.clone(),
            argv,
            env,
            credential: Credential { files },
            sockets: sockets.clone(),
            mounts,
            limits: Default::default(),
        };
        // The image unpacks first: half a minute or more, with no line of the job.
        self.live.as_ref().inspect(|_| eprintln!("podrunner: {}: unit {id} starts (its image unpacks)", job.as_str()));
        let started = self.driver.start(&spec).map_err(|e| start_error(&id, &e.to_string()));
        if let Err(e) = started {
            if let Some(h) = hooks {
                h.stop();
            }
            end_sockets(&sockets);
            if let Some(w) = &work {
                let _ = crate::work::clear(w.parent().unwrap_or(w));
            }
            return Err(e);
        }
        crate::enter::register(job.as_str(), self.driver.clone(), &id);
        // Wait in slices: a signal destroys the unit at once (T-141).
        let until = Instant::now() + limit;
        let reach_by = Instant::now() + self.reach;
        let mut unreached = false;
        let waited = loop {
            let slice = until.saturating_duration_since(Instant::now()).min(Duration::from_secs(1));
            pump(&self.live, job, &broker);
            match self.driver.wait(&id, slice) {
                Ok(None) if Instant::now() >= reach_by && broker.seen().is_empty() => {
                    unreached = true;
                    break Ok(None);
                }
                Ok(None) if !crate::cancel::stopped(&self.stop) && Instant::now() < until => continue,
                other => break other,
            }
        };
        let cancelled = crate::cancel::stopped(&self.stop) && matches!(waited, Ok(None));
        // Still running at the limit: end the unit (destroy below).
        let timed_out = matches!(waited, Ok(None)) && !unreached;
        let t = Instant::now();
        while !cancelled && broker.completions().is_empty() && t.elapsed() < self.grace {
            std::thread::sleep(Duration::from_millis(50));
        }
        // No result: keep the end of the output of the unit, before it goes.
        let output = match broker.completions().is_empty() {
            true => self.driver.unit_logs(&id).map(|o| tail(&o)).ok(),
            false => None,
        };
        // The hosts that the egress policy of the unit refused, before the
        // grant ends: the log names them.
        let denied = self.egress.as_ref().map(|(e, _)| e.denials(id.as_str())).unwrap_or_default();
        let (driver, name, unit) = (self.driver.clone(), job.as_str().to_string(), id.clone());
        let destroy = move || {
            crate::enter::forget(&name);
            driver.destroy(&unit).map_err(|e| format!("destroy {unit}: {e}"))
        };
        // At a cancel, the end of a unit says that the run waits (T-636),
        // with a bound (T-716, T-720, T-721).
        let ending = (matches!(waited, Ok(None)), crate::cancel::stopped(&self.stop), self.bound);
        let (residue, refusals) = stopping::end_unit((job.as_str(), id.as_str()), ending, destroy, hooks);
        end_sockets(&sockets);
        // The workspaces of the job, also of a failed one (T-138).
        let exported = match (&work, &self.export) {
            (Some(w), Some(dir)) => {
                let r = crate::work::export(w, &dir.join(job.as_str()))
                    .inspect_err(|e| eprintln!("podrunner: {}: --export: {e}", job.as_str()));
                let _ = crate::work::clear(w.parent().unwrap_or(w));
                r.err()
            }
            _ => None,
        };
        let residue = residue.transpose()?;
        let mut log = record(&self.live, &mut self.blobs, job, &broker);
        let ran = log.len();
        for r in &refusals {
            log.push_str(&format!("\npodrunner: {r}"));
        }
        if let Some(o) = output {
            log.push_str(&format!("\npodrunner: the end of the output of unit {id}:\n{o}"));
        }
        if let Some(e) = exported {
            log.push_str(&format!("\npodrunner: {e}"));
        }
        let late = format!("unit {id} did not go in {} s at the cancel", self.bound.as_secs());
        if residue.is_none() {
            log.push_str(&format!("\npodrunner: {late}; its end goes on in the background"));
        }
        let failed = !broker.completions().iter().any(|c| c.conclusion == "succeeded");
        proxy::denials(&mut log, job.as_str(), id.as_str(), &denied, failed);
        // A job that did not succeed: the last requests of its broker.
        if !broker.completions().iter().any(|c| c.conclusion == "succeeded") {
            let seen = broker.seen();
            let last = &seen[seen.len().saturating_sub(15)..];
            log.push_str(&format!("\npodrunner: the last requests of the broker: {}", last.join(", ")));
        }
        added(&mut self.blobs, job, &log[ran..]);
        self.logs.push((job.clone(), log));
        let exit = waited.map_err(|e| format!("wait {id}: {e}"))?;
        if cancelled {
            return Ok(End { result: JobResult::Cancelled, outputs: vec![] });
        }
        if unreached {
            return Err(format!(
                "the runner of unit {id} sent no request to its broker in {} s: it cannot reach the broker; \
                 the end of its output is in the log",
                self.reach.as_secs()
            ));
        }
        if timed_out {
            return Err(format!("the job ran past its timeout-minutes ({} s); unit {id} was ended", limit.as_secs()));
        }
        match residue {
            None => return Err(late),
            Some(r) if !r.is_clean() => return Err(format!("unit {id} left residue: {r:?}")),
            Some(_) => {}
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

#[cfg(test)]
mod cancel_tests;
#[cfg(test)]
mod tests;
