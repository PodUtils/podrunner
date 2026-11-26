//! The hooks of one unit on its driver (GitHub #35): each call that passed
//! the checks becomes container operations of `UnitDriver`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use podrunner_schema::RefusalCode;
use podrunner_unit::{ContainerId, ContainerMount, ContainerSpec, ExecSpec, Exit, UnitCode, UnitDriver, UnitError};
use serde_json::{Value, json};

use crate::input::HookInput;
use crate::policy::{Gate, Plan, Planned, Policy, Refusal};
use crate::server::{Answer, JobEvent, Unit};

/// The `PATH` after the prepended paths of a step.
const BASE_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Where each container of the job sees the CA bundle of the broker of its
/// unit (T-578): a directory of its own, read-only, outside each mount
/// table of the runner.
pub const CA_DIR: &str = "/__podrunner";

/// The CA bundle of the broker of a unit (T-578): a container of the job
/// trusts it as the unit does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitCa {
    /// A host directory that holds the bundle and nothing of the job.
    pub dir: PathBuf,
    /// The name of the bundle in `dir`.
    pub file: String,
    /// The variables that name the bundle (`ca::env` of `pod-broker`).
    pub vars: Vec<String>,
}

/// The cache of the job of a unit (T-651): each container of the job sees
/// it where the unit sees it, with the same mode and variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitCache {
    /// The directory of the cache on the host.
    pub dir: PathBuf,
    /// Where the unit and each container see it.
    pub target: String,
    pub read_only: bool,
    /// The variables of the cache in the unit.
    pub env: std::collections::BTreeMap<String, String>,
}

/// The hooks of one unit.
pub struct HookUnit {
    driver: Arc<dyn UnitDriver>,
    unit: podrunner_unit::UnitId,
    policy: Policy,
    /// The runner directory of the unit on the host: `policy.work_root` in
    /// the unit. When it is not given, the driver tells it at the first
    /// call (`observe`), and an empty `work_root` is the same path.
    host_root: Option<PathBuf>,
    /// A host directory for the output of each step.
    logs: PathBuf,
    gate: Gate,
    /// The CA bundle of the broker, when the broker ends the TLS of the
    /// job.
    ca: Option<UnitCa>,
    /// The cache of the job, when it has one.
    cache: Option<UnitCache>,
    job: Option<ContainerId>,
    services: Vec<ContainerId>,
    steps: u64,
    /// The job hooks of the runner, in order.
    pub events: Vec<JobEvent>,
    /// Each refusal, for an audit line `hook-refuse` (the code only).
    pub refusals: Vec<RefusalCode>,
    /// The text of each error of the driver behind a refusal: the runner gets
    /// the code only, the log of a local run names the cause.
    pub driver_errors: Vec<String>,
    /// The digest of each image that a container started with, once
    /// (`job_images` of the run report).
    pub images: Vec<String>,
}

impl HookUnit {
    pub fn new(
        driver: Arc<dyn UnitDriver>,
        unit: podrunner_unit::UnitId,
        policy: Policy,
        host_root: Option<PathBuf>,
        logs: PathBuf,
    ) -> HookUnit {
        let gate = Gate::default();
        HookUnit {
            driver,
            unit,
            policy,
            host_root,
            logs,
            gate,
            ca: None,
            cache: None,
            job: None,
            services: vec![],
            steps: 0,
            events: vec![],
            refusals: vec![],
            driver_errors: vec![],
            images: vec![],
        }
    }

    /// Give each container of the job the cache of the job (T-651). The
    /// slots decide it after the socket opens, before the unit starts.
    pub fn set_cache(&mut self, cache: Option<UnitCache>) {
        self.cache = cache;
    }

    /// Give each container of the job the CA bundle `ca` of the broker of
    /// the unit.
    pub fn with_ca(mut self, ca: Option<UnitCa>) -> HookUnit {
        self.ca = ca;
        self
    }

    /// Keep the digest of the image of `p` (the checks pinned it).
    fn note_image(&mut self, p: &Planned) {
        if let Ok((_, digest)) = podrunner_unit::parse_pinned(&p.image)
            && !self.images.iter().any(|i| i == digest)
        {
            self.images.push(digest.to_string());
        }
    }

    fn spec(&self, p: &Planned) -> Result<ContainerSpec, UnitError> {
        let root = self.host_root.clone().unwrap_or_default();
        let mut mounts = Vec::new();
        for m in &p.mounts {
            // The job writes the tree under the root: a link in a source
            // would give a container a path of the host (T-403). A driver
            // with a root of its own (no host root here) checks the sources
            // under that root: a walk here would read the working directory.
            let mut at = root.clone();
            let parts = if root.as_os_str().is_empty() { None } else { Some(std::path::Path::new(&m.source)) };
            for part in parts.into_iter().flat_map(|p| p.components()) {
                at.push(part);
                if std::fs::symlink_metadata(&at).is_ok_and(|md| md.file_type().is_symlink()) {
                    return Err(UnitError::new(
                        UnitCode::ContainerOptionRefused,
                        format!("the mount source {} holds a symbolic link", m.source),
                    ));
                }
            }
            mounts.push(ContainerMount {
                source: root.join(&m.source),
                target: m.target.clone(),
                read_only: m.read_only,
            });
        }
        let mut env = p.env.clone();
        // The host made the directory of the bundle: it is not a path of the
        // job, so it has no walk for a link. A value of the job keeps its
        // priority, as in the unit.
        if let Some(ca) = &self.ca {
            mounts.push(ContainerMount { source: ca.dir.clone(), target: CA_DIR.into(), read_only: true });
            let path = format!("{CA_DIR}/{}", ca.file);
            for v in &ca.vars {
                env.entry(v.clone()).or_insert_with(|| path.clone());
            }
        }
        // The host made the cache and checked it: no walk for a link. The
        // mode of the unit holds in each container; a value of the job
        // keeps its priority.
        if let Some(c) = &self.cache {
            mounts.push(ContainerMount { source: c.dir.clone(), target: c.target.clone(), read_only: c.read_only });
            for (k, v) in &c.env {
                env.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        Ok(ContainerSpec {
            image: p.image.clone(),
            argv: p.argv.clone(),
            env,
            workdir: p.workdir.clone(),
            mounts,
            options: p.options.clone(),
        })
    }

    fn log(&mut self) -> PathBuf {
        self.steps += 1;
        self.logs.join(format!("step-{}.log", self.steps))
    }

    fn run(&mut self, plan: Plan) -> Result<Answer, UnitError> {
        let d = self.driver.clone();
        match plan {
            Plan::Prepare { job, services } => {
                let mut ctx_job = Value::Null;
                let mut alpine = false;
                if let Some(mut p) = job {
                    // Runner v2.338.2 sends the job container with no entry
                    // point: the container only has to live while the steps
                    // run in it, as with the docker hooks of `actions`.
                    if p.argv.is_empty() {
                        p.argv = ["tail", "-f", "/dev/null"].map(String::from).to_vec();
                    }
                    // A start that fails, or a container that stops at once
                    // (an image with no `tail`), is a start failure that
                    // names what a job container needs (#278).
                    let start = |e: UnitError| match e.code {
                        UnitCode::PayloadNotPinned | UnitCode::ArchMismatch | UnitCode::ContainerOptionRefused => e,
                        _ => UnitError::new(UnitCode::UnitStartFailed, e.detail),
                    };
                    let id = d.container_start(&self.unit, &self.spec(&p)?).map_err(start)?;
                    self.note_image(&p);
                    for _ in 0..4 {
                        // An exit, not "not running": a container that is
                        // still being created is not running yet.
                        if d.container_inspect(&self.unit, &id).is_ok_and(|s| s.exit.is_some()) {
                            let _ = d.container_stop(&self.unit, &id);
                            return Err(UnitError::new(UnitCode::UnitStartFailed, "the job container stopped"));
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    let probe = ExecSpec {
                        argv: vec!["test".into(), "-f".into(), "/etc/alpine-release".into()],
                        ..Default::default()
                    };
                    alpine = d.container_exec(&self.unit, &id, &probe)? == Exit::Code(0);
                    ctx_job = json!({ "id": id.0, "network": "podrunner", "ports": {} });
                    self.job = Some(id);
                }
                let mut ctx_services = Vec::new();
                for p in services {
                    let id = d.container_start(&self.unit, &self.spec(&p)?)?;
                    self.note_image(&p);
                    ctx_services.push(json!({ "id": id.0, "ports": {} }));
                    self.services.push(id);
                }
                let response = json!({
                    "state": {},
                    "context": { "container": ctx_job, "services": ctx_services },
                    "isAlpine": alpine,
                });
                Ok(Answer { exit: 0, output: response.to_string().into_bytes() })
            }
            Plan::Script(s) => {
                let Some(job) = self.job.clone() else {
                    return Err(UnitError::new(UnitCode::ContainerNotSupported, "no job container"));
                };
                let mut env = s.environment_variables.clone();
                if !s.prepend_path.is_empty() {
                    env.insert("PATH".into(), format!("{}:{BASE_PATH}", s.prepend_path.join(":")));
                }
                let mut argv: Vec<String> = s.entry_point.into_iter().collect();
                argv.extend(s.entry_point_args);
                let log = self.log();
                let spec = ExecSpec { argv, env, workdir: s.working_directory, stdin: false, log: Some(log.clone()) };
                let exit = d.container_exec(&self.unit, &job, &spec)?;
                let output = std::fs::read(&log).unwrap_or_default();
                let _ = std::fs::remove_file(&log);
                Ok(Answer { exit: code(exit), output })
            }
            Plan::ContainerStep(p) => {
                let id = d.container_start(&self.unit, &self.spec(&p)?)?;
                self.note_image(&p);
                let exit = loop {
                    if let Some(exit) = d.container_inspect(&self.unit, &id)?.exit {
                        break exit;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                };
                let output = d.container_logs(&self.unit, &id).unwrap_or_default();
                let _ = d.container_stop(&self.unit, &id);
                Ok(Answer { exit: code(exit), output })
            }
            Plan::Cleanup => {
                for id in self.job.take().into_iter().chain(std::mem::take(&mut self.services)) {
                    let _ = d.container_stop(&self.unit, &id);
                }
                Ok(Answer::default())
            }
        }
    }
}

/// The exit code of the hook script: a signal is 128 and its number.
fn code(e: Exit) -> i32 {
    match e {
        Exit::Code(c) => c,
        Exit::Signal(s) => 128 + s,
    }
}

/// A refusal of the driver, by its code, with no text of the driver.
fn refusal(e: &UnitError) -> Refusal {
    let (code, why) = match e.code {
        UnitCode::PayloadNotPinned => (RefusalCode::PayloadNotPinned, "the driver refused the image"),
        UnitCode::ContainerOptionRefused => (RefusalCode::ContainerOptionRefused, "the driver refused an option"),
        UnitCode::ArchMismatch => (RefusalCode::ArchMismatch, "the image is of another platform"),
        UnitCode::UnitStartFailed => (
            RefusalCode::ContainerNotSupported,
            "a container did not start, or it stopped (at its start or later): a job container must live while its steps \
             run; with no entry point it runs `tail -f /dev/null`, as on GitHub, so its image needs `tail` and \
             `sh`; the cause of the driver is in the audit",
        ),
        _ => (RefusalCode::ContainerNotSupported, "the driver did not do the container operation"),
    };
    Refusal { code, why }
}

impl Unit for HookUnit {
    fn hook(&mut self, input: HookInput) -> Result<Answer, Refusal> {
        // A driver that mounts the socket directory into its unit runs the
        // unit in a root of its own (podvm): it resolves a relative mount
        // source under the runner directory in that root, so the sources
        // stay relative. Another driver shares the paths of the host.
        let own_root = self.driver.socket_mount().is_some();
        if self.host_root.is_none() && !own_root {
            self.host_root = self.driver.observe(&self.unit).ok().and_then(|o| o.runner_root);
            if self.policy.work_root.is_empty() {
                self.policy.work_root = self.host_root.as_ref().map(|r| r.display().to_string()).unwrap_or_default();
            }
        }
        if (self.host_root.is_none() && !own_root) || self.policy.work_root.is_empty() {
            self.refusals.push(RefusalCode::ContainerNotSupported);
            return Err(Refusal { code: RefusalCode::ContainerNotSupported, why: "the unit has no runner directory" });
        }
        let mut driver_error = None;
        let result = self.gate.check(&input, &self.policy).and_then(|plan| {
            self.run(plan).map_err(|e| {
                driver_error = Some(format!("{:?}: {}", e.code, e.detail));
                refusal(&e)
            })
        });
        self.driver_errors.extend(driver_error);
        if let Err(r) = &result {
            self.refusals.push(r.code);
        }
        result
    }

    fn job_state(&mut self, event: JobEvent) -> Result<(), Refusal> {
        // A second job-started is ignored, and slots audits it as a
        // refusal; the step of the runner goes on.
        if event == JobEvent::JobStarted && self.events.contains(&JobEvent::JobStarted) {
            self.refusals.push(RefusalCode::ContainerOptionRefused);
            return Ok(());
        }
        self.events.push(event);
        Ok(())
    }
}
