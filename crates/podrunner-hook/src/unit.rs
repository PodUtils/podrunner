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
    job: Option<ContainerId>,
    services: Vec<ContainerId>,
    steps: u64,
    /// The job hooks of the runner, in order.
    pub events: Vec<JobEvent>,
    /// Each refusal, for an audit line `hook-refuse` (the code only).
    pub refusals: Vec<RefusalCode>,
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
            job: None,
            services: vec![],
            steps: 0,
            events: vec![],
            refusals: vec![],
        }
    }

    fn spec(&self, p: &Planned) -> ContainerSpec {
        let mounts = p
            .mounts
            .iter()
            .map(|m| ContainerMount {
                source: self.host_root.clone().unwrap_or_default().join(&m.source),
                target: m.target.clone(),
                read_only: m.read_only,
            })
            .collect();
        ContainerSpec {
            image: p.image.clone(),
            argv: p.argv.clone(),
            env: p.env.clone(),
            workdir: p.workdir.clone(),
            mounts,
            options: p.options.clone(),
        }
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
                    let id = d.container_start(&self.unit, &self.spec(&p))?;
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
                    let id = d.container_start(&self.unit, &self.spec(&p))?;
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
                let id = d.container_start(&self.unit, &self.spec(&p))?;
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
        _ => (RefusalCode::ContainerNotSupported, "the driver did not do the container operation"),
    };
    Refusal { code, why }
}

impl Unit for HookUnit {
    fn hook(&mut self, input: HookInput) -> Result<Answer, Refusal> {
        if self.host_root.is_none() {
            self.host_root = self.driver.observe(&self.unit).ok().and_then(|o| o.runner_root);
            if self.policy.work_root.is_empty() {
                self.policy.work_root = self.host_root.as_ref().map(|r| r.display().to_string()).unwrap_or_default();
            }
        }
        if self.host_root.is_none() || self.policy.work_root.is_empty() {
            self.refusals.push(RefusalCode::ContainerNotSupported);
            return Err(Refusal { code: RefusalCode::ContainerNotSupported, why: "the unit has no runner directory" });
        }
        let result = self.gate.check(&input, &self.policy).and_then(|plan| self.run(plan).map_err(|e| refusal(&e)));
        if let Err(r) = &result {
            self.refusals.push(r.code);
        }
        result
    }

    fn job_state(&mut self, event: JobEvent) -> Result<(), Refusal> {
        if event == JobEvent::JobStarted && self.events.contains(&JobEvent::JobStarted) {
            return Err(Refusal { code: RefusalCode::ContainerOptionRefused, why: "job-started comes once" });
        }
        self.events.push(event);
        Ok(())
    }
}
