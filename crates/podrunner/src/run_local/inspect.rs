//! `podrunner run --list`, `--graph`, `--validate` and `--dry-run` (T-128):
//! what a run would do, with no unit. A dry run builds the job message of
//! each cell, so each refusal (an action or an image with no pin, a
//! reusable workflow) and each missing pin shows before a job starts.

use std::time::Duration;

use pod_actions_workflow::{Graph, JobResult, Kind, Node, Workflow};
use podrunner_local::Job;
use podrunner_local::drive::{End, Executor};
use podrunner_schema::JobId;

/// The labels of `runs-on`, as text.
fn runs_on(n: Option<&Node>) -> String {
    match n.map(|n| &n.kind) {
        Some(Kind::String(s)) => s.clone(),
        Some(Kind::Seq(items)) => items
            .iter()
            .filter_map(|i| match &i.kind {
                Kind::String(s) => Some(s.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(", "),
        Some(Kind::Map(_)) => "(a group)".into(),
        _ => "-".into(),
    }
}

/// Each job: its id, its name, whether it has a matrix, its `runs-on` and
/// its needs.
fn rows(w: &Workflow) -> Vec<(String, String, bool, String, Vec<String>)> {
    let row = |j: &pod_actions_workflow::Job| {
        let name = j.name.clone().unwrap_or_else(|| j.id.clone());
        let matrix = j.strategy.as_ref().and_then(|s| s.matrix.as_ref()).is_some();
        (j.id.clone(), name, matrix, runs_on(j.runs_on.as_ref()), j.needs.clone())
    };
    w.jobs.iter().map(row).collect()
}

/// One line for each job: its id, its name, its `runs-on` and its needs.
pub(super) fn list(w: &Workflow) -> String {
    let mut out = String::new();
    for (id, name, matrix, runs_on, needs) in rows(w) {
        let needs = if needs.is_empty() { "-".to_string() } else { needs.join(", ") };
        let matrix = if matrix { " (a matrix)" } else { "" };
        out.push_str(&format!("{id}\t{name}{matrix}\truns-on: {runs_on}\tneeds: {needs}\n"));
    }
    out
}

/// The stages of `needs` (a job is one stage after its last need), and
/// each edge.
type Stages = (Vec<Vec<String>>, Vec<(String, String)>);

fn stages(w: &Workflow) -> Result<Stages, String> {
    let g = Graph::new(w).map_err(|e| e.to_string())?;
    let mut stage: std::collections::BTreeMap<String, usize> = Default::default();
    let mut stages: Vec<Vec<String>> = Vec::new();
    for job in g.order() {
        let s = g.needs(job).iter().map(|n| stage[*n] + 1).max().unwrap_or(0);
        stage.insert(job.to_string(), s);
        if stages.len() <= s {
            stages.resize(s + 1, Vec::new());
        }
        stages[s].push(job.to_string());
    }
    Ok((stages, g.edges().into_iter().map(|(n, j)| (n.to_string(), j.to_string())).collect()))
}

/// The stages and the edges as text.
pub(super) fn graph(w: &Workflow) -> Result<String, String> {
    let (stages, edges) = stages(w)?;
    let mut out = String::new();
    for (i, jobs) in stages.iter().enumerate() {
        out.push_str(&format!("stage {}: {}\n", i + 1, jobs.join(", ")));
    }
    for (need, job) in edges {
        out.push_str(&format!("{need} -> {job}\n"));
    }
    Ok(out)
}

/// `--validate`, `--list` and `--graph` of one file. `--validate` checks the
/// needs as a plan reads them: a need that is not a job, or a cycle (T-209).
/// With `--json`, each view is one JSON object on stdout (T-323 F16).
pub(super) fn answer(a: &super::args::Args, text: &str) -> super::Out {
    use serde_json::json;
    // `--validate --json` also gives an object for a file that is not valid
    // (T-348).
    let fail = |e: String| {
        let mut out = super::fail(super::Class::Config, format!("{}: {e}", a.file.display()));
        if a.validate && a.json {
            out.stdout = format!(
                "{}\n",
                json!({"file": a.file.display().to_string(), "valid": false, "warnings": [], "error": e})
            );
        }
        out
    };
    let p = match pod_actions_workflow::parse(text) {
        Ok(p) => p,
        Err(e) => return fail(e.to_string()),
    };
    let mut stdout = String::new();
    // A cycle of needs fails each view, as a plan refuses it (T-279).
    if let Err(e) = Graph::new(&p.workflow) {
        return fail(e.to_string());
    }
    if let Err(e) = filter(&p.workflow, &a.jobs, &a.matrix) {
        return super::fail(super::Class::Usage, e);
    }
    let file = a.file.display().to_string();
    if a.validate {
        // Each cron of on.schedule, as `--schedule` reads it (T-279).
        if let Err(e) = super::watch::crons(&p.workflow.on) {
            return fail(e);
        }
        for n in super::watch::notes(&p.workflow.on) {
            eprintln!("podrunner: {file}: {n}");
        }
        // Each number of minutes, as a run reads it (T-339).
        let long = match minutes(&p.workflow) {
            Ok(long) => long,
            Err(e) => return fail(e),
        };
        let warnings: Vec<String> = p.warnings.iter().map(|w| w.to_string()).chain(long).collect();
        if a.json {
            stdout.push_str(&format!("{}\n", json!({"file": file, "valid": true, "warnings": warnings})));
        } else {
            for w in &warnings {
                stdout.push_str(&format!("warning: {w}\n"));
            }
            stdout.push_str(&format!("valid: {file}\n"));
        }
    }
    if a.list && a.json {
        for (id, name, matrix, runs_on, needs) in rows(&p.workflow) {
            let row = json!({"job": id, "name": name, "matrix": matrix, "runs_on": runs_on, "needs": needs});
            stdout.push_str(&format!("{row}\n"));
        }
    } else if a.list {
        stdout.push_str(&list(&p.workflow));
    }
    if a.graph {
        let out = match a.json {
            true => stages(&p.workflow).map(|(s, e)| format!("{}\n", json!({"stages": s, "edges": e}))),
            false => graph(&p.workflow),
        };
        match out {
            Ok(g) => stdout.push_str(&g),
            Err(e) => return fail(e),
        }
    }
    super::Out { code: 0, stdout, stderr: String::new() }
}

/// Each `--job` names a job of the workflow, and each `--matrix KEY` an
/// axis of one of its matrices, also for `--list` (T-634). A matrix or an
/// `include` that is an expression, or a call of a workflow, can have each
/// axis: no refusal then.
pub(super) fn filter(w: &Workflow, jobs: &[String], matrix: &[(String, String)]) -> Result<(), String> {
    if let Some(j) = jobs.iter().find(|j| w.job(j).is_none()) {
        let ids: Vec<&str> = w.jobs.iter().map(|j| j.id.as_str()).collect();
        return Err(format!("--job {j}: the workflow has no such job (its jobs: {ids:?})"));
    }
    let mut axes = std::collections::BTreeSet::new();
    for j in &w.jobs {
        let m = j.strategy.as_ref().and_then(|s| s.matrix.as_ref()).map(|m| &m.kind);
        let m = match (m, &j.uses) {
            (_, Some(_)) => return Ok(()),
            (None, None) => continue,
            (Some(Kind::Map(m)), None) => m,
            (Some(_), None) => return Ok(()),
        };
        for (k, v) in m {
            match (k.name.as_str(), &v.kind) {
                ("exclude", _) => {}
                ("include", Kind::Seq(items)) => {
                    for i in items {
                        let Kind::Map(e) = &i.kind else { return Ok(()) };
                        axes.extend(e.iter().map(|(k, _)| k.name.clone()));
                    }
                }
                ("include", _) => return Ok(()),
                (key, _) => {
                    axes.insert(key.to_string());
                }
            }
        }
    }
    match matrix.iter().find(|(k, _)| !axes.contains(k)) {
        Some((k, v)) => {
            Err(format!("--matrix {k}={v}: no matrix of the workflow has the axis {k} (its axes: {axes:?})"))
        }
        None => Ok(()),
    }
}

/// Each `timeout-minutes` and `cancel-timeout-minutes` of a job and
/// `timeout-minutes` of a step: a number, or an expression that the run
/// evaluates. A warning for each `timeout-minutes` of a job above the 5
/// days at which the run stops it (T-634).
fn minutes(w: &Workflow) -> Result<Vec<String>, String> {
    let check = |n: &Node, at: String| match &n.kind {
        Kind::String(v) if !v.contains("${{") && v.trim().parse::<f64>().is_err() => {
            Err(format!("{at} (line {}): {v:?} is not a number of minutes", n.mark.line))
        }
        _ => Ok(()),
    };
    let max = podrunner_local::drive::MAX_MINUTES;
    let mut long = Vec::new();
    for j in &w.jobs {
        if let Some(n) = &j.timeout_minutes {
            let v = match &n.kind {
                Kind::Number(v) => Some(*v),
                Kind::String(v) => v.trim().parse::<f64>().ok(),
                _ => None,
            };
            if let Some(v) = v.filter(|v| *v > max) {
                long.push(format!(
                    "jobs.{}.timeout-minutes (line {}): {v} is more than {max} (5 days, the limit of a job of a \
                     self-hosted runner); the run stops the job at 5 days",
                    j.id, n.mark.line
                ));
            }
        }
        for (key, n) in [("timeout-minutes", &j.timeout_minutes), ("cancel-timeout-minutes", &j.cancel_timeout_minutes)]
        {
            if let Some(n) = n {
                check(n, format!("jobs.{}.{key}", j.id))?;
            }
        }
        for (i, s) in j.steps.iter().enumerate() {
            if let Some(n) = &s.timeout_minutes {
                check(n, format!("jobs.{}.steps[{i}].timeout-minutes", j.id))?;
            }
        }
    }
    Ok(long)
}

/// The warnings of `minutes` for a run; a value that is no number is for
/// the run to refuse, not here.
pub(super) fn long_timeouts(w: &Workflow) -> Vec<String> {
    minutes(w).unwrap_or_default()
}

/// The text of a workflow file that cannot be read: for a file that is not
/// there, the workflows of its directory (the quick start names `ci.yml`,
/// which a project may not have; T-331).
pub(super) fn missing(file: &std::path::Path, e: &std::io::Error) -> String {
    if e.kind() != std::io::ErrorKind::NotFound {
        return format!("{}: {e}", file.display());
    }
    let dir = file.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".yml") || n.ends_with(".yaml"))
        .collect();
    names.sort();
    match names.is_empty() {
        true => format!("{}: no such file, and {} has no workflow", file.display(), dir.display()),
        false => format!("{}: no such file; the workflows of {}: {}", file.display(), dir.display(), names.join(", ")),
    }
}

/// The refusal of a job with containers where podvm gives no container
/// hooks (T-311): `doctor --run` names the hooks of podvm.
pub(super) const NO_HOOKS: &str = "the job needs containers, and podvm gives no container hooks on this host (`podrunner doctor --run` names it); run it with --microvm and --engine (a host with /dev/kvm)";

/// The executor of a dry run: each job message is built, and no unit runs.
/// On a runner directory only, a job with containers fails as its run
/// would (T-218).
pub(super) struct DryRun {
    pub host: bool,
    /// A podvm unit: its hook server refuses `services:` (T-272).
    pub podvm: bool,
    /// podvm gives no container hooks on this host (T-311).
    pub no_hooks: bool,
    /// The store of the actions and the pins, to find a Docker action (T-270).
    pub actions: Option<std::path::PathBuf>,
    pub pins: std::collections::BTreeMap<String, String>,
    /// The tarball of the tree of `--checkout`, for a `./` action.
    pub tree: Option<std::path::PathBuf>,
    /// The labels of `--platform`.
    pub platforms: Vec<String>,
}

impl Executor for DryRun {
    // A label that the run refuses, the dry run refuses (T-289); a label
    // that a --platform names is routed by the run.
    fn select(&mut self, labels: &[String]) -> Result<(), String> {
        match labels.iter().any(|l| self.platforms.iter().any(|p| p.eq_ignore_ascii_case(l))) {
            true => Ok(()),
            false => podrunner_local::dispatch::foreign(labels),
        }
    }

    fn run(&mut self, _job: &JobId, message: Job, _limit: Duration) -> Result<End, String> {
        if self.host && podrunner_local::exec::needs_containers(&message) {
            return Err("the job needs containers: run it in a unit (--podvm, or --microvm with --engine), not on a runner directory".into());
        }
        if self.podvm && self.no_hooks && podrunner_local::exec::needs_containers(&message) {
            return Err(NO_HOOKS.into());
        }
        let src = podrunner_local::exec::docker::Sources {
            store: self.actions.as_deref(),
            pins: &self.pins,
            tree: self.tree.as_deref(),
        };
        if let Some(e) = podrunner_local::exec::docker::build_refusal(&message, &src).filter(|_| self.podvm) {
            return Err(e);
        }
        if self.podvm && message.overrides.contains_key("jobServiceContainers") {
            return Err("the job has services: a podvm unit of the local mode is on the host network, so it runs no services; run it with --microvm and --engine".into());
        }
        if self.host
            && let Some(e) = podrunner_local::exec::docker::refusal(
                &message,
                &podrunner_local::exec::docker::Sources {
                    store: self.actions.as_deref(),
                    pins: &self.pins,
                    tree: self.tree.as_deref(),
                },
            )
        {
            return Err(e);
        }
        Ok(End { result: JobResult::Success, outputs: vec![] })
    }
}
