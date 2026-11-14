//! `podrunner run FILE` (T-089, the local mode of T-046): run the jobs of
//! one workflow file, with no forge. The local source (`podrunner-local`)
//! orders and expands the jobs; the official runner runs each one against
//! a fake broker on the loopback: in DIR with `--runner-dir`, or in a fresh
//! podvm unit with `--podvm` (T-090). One line for each job; the exit code is 0 when no job failed,
//! 1 when one failed, and the code of the class of a usage or a file error.

mod actions;
mod args;
mod context;
mod guest;
mod inspect;
mod logs;
mod routes;

use std::path::PathBuf;

use pod_actions_expr::Value;
use pod_actions_workflow::{Base, JobResult};
use pod_core::failure::Class;
use podrunner_local::LocalSource;
use podrunner_local::drive::{Line, drive};
use podrunner_local::exec::InUnit;

use crate::cli::Out;

use args::{Args, parse};

pub const USAGE: &str = "usage: podrunner run FILE (--runner-dir DIR | (--podvm PATH | --microvm PATH [--engine DIR] | --incus PATH) --images STORE --image NAME@sha256:HEX)\n         [--podnet PATH] [--allow HOST]... [--platform LABEL=NAME@sha256:HEX|host]... [--actions DIR] [--offline | --frozen] [--artifacts DIR] [--cache DIR | --no-cache] [--secret NAME]... [--secret-file FILE] [--github-token-env NAME] [--job ID]... [--matrix KEY=VALUE]... [--list | --graph | --validate | --dry-run] [-q] [--json] [--log-dir DIR] [--checkout DIR [--checkout-head] [--with-git]] [--event NAME] [--event-path FILE] [--force-event] [--remote NAME] [--ref REF] [--sha SHA] [--repository OWNER/REPO] [--actor NAME] [--var NAME=VALUE]... [--input NAME=VALUE]... [--var-file FILE] [--input-file FILE] [--env NAME=VALUE]... [--env-file FILE]";

fn fail(class: Class, msg: impl Into<String>) -> Out {
    Out { code: class.exit_code(), stdout: String::new(), stderr: format!("podrunner: {}\n", msg.into()) }
}

fn object(pairs: &[(String, String)]) -> Value {
    Value::object(pairs.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))))
}

fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" }
}

/// The egress policy of each unit of a held run: only the hosts of
/// `--allow` (`broker-policy.v1`).
fn policy(allow: &[String]) -> String {
    let hosts: Vec<serde_json::Value> = allow.iter().map(|h| serde_json::json!({ "host": h })).collect();
    serde_json::json!({ "schema": 1, "mode": "allow", "allow": hosts }).to_string()
}

/// The executor of `--podvm`: each job in a fresh podvm unit of the
/// runner image (T-090), with the state of the units in `state`.
fn in_podvm(a: &Args, state: &std::path::Path) -> Result<InUnit, Out> {
    use podrunner_podvm::{Podvm, PodvmUnit};
    use podrunner_unit::UnitDriver;
    let (Some(binary), Some(images), Some(image)) = (&a.podvm, &a.images, &a.image) else {
        return Err(fail(Class::Usage, USAGE));
    };
    let podvm = Podvm { binary: binary.clone() };
    let mut unit = PodvmUnit::new(podvm, state, images).map_err(|e| fail(Class::Config, format!("podvm: {e}")))?;
    let mut egress = None;
    if let Some(podnet) = &a.podnet {
        unit = unit.with_podnet(podnet).map_err(|e| fail(Class::Unsupported, format!("--podnet: {e}")))?;
        let hold = unit.probe().map_err(|e| fail(Class::Unavailable, format!("probe: {e}")))?.egress_hold;
        // The fake broker listens on the loopback of the host: the seccomp
        // hold lets a unit reach it; the netns hold does not (T-095).
        if hold != podrunner_schema::EgressHold::Advisory {
            return Err(fail(
                Class::Unsupported,
                format!("--podnet: the hold is {hold:?}; the local mode takes only the seccomp hold (advisory) now"),
            ));
        }
        let e = podrunner_egress::Egress::host(None, true, podrunner_egress::dns::resolver(false, Vec::new()))
            .map_err(|e| fail(Class::Unavailable, format!("the egress module: {e}")))?;
        egress = Some((std::sync::Arc::new(e), policy(&a.allow)));
    }
    let platform = if arch() == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let prepared = unit.prepare(image, platform, false).map_err(|e| fail(Class::Config, format!("{image}: {e}")))?;
    let mut exec = InUnit::new(std::sync::Arc::new(unit), prepared);
    exec.egress = egress;
    Ok(exec)
}

/// The line of one job.
pub fn line(l: &Line) -> String {
    match l {
        Line::Ran { name, result: JobResult::Success, .. } => format!("ok      {name}\n"),
        Line::Ran { name, result, .. } => format!("{:<7} {name}\n", result.as_str()),
        Line::Refused { name, reason, .. } => format!("failure {name}: {reason}\n"),
        Line::Skipped { job, condition } => format!("skipped {job}: if: {condition}\n"),
        Line::NotSelected { name, reason, .. } => format!("skipped {name}: {reason}\n"),
    }
}

/// The secrets of the run (T-121) and the token of `github.token` (T-122):
/// `.secrets` of the checkout when it exists, then `--secret-file`, then
/// each `--secret`.
/// The vars, the inputs and the env of the run: each file, then each flag
/// over it (T-126).
type Pairs = Vec<(String, String)>;

fn files(a: &Args) -> Result<(Pairs, Pairs, Pairs), String> {
    let read = |f: &Option<PathBuf>, flags: &[(String, String)]| -> Result<Pairs, String> {
        let mut out = match f {
            Some(p) => {
                let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
                podrunner_local::secrets::parse_pairs(&text).map_err(|e| format!("{}: {e}", p.display()))?
            }
            None => Vec::new(),
        };
        for (k, v) in flags {
            out.retain(|(o, _)| o != k);
            out.push((k.clone(), v.clone()));
        }
        Ok(out)
    };
    Ok((read(&a.var_file, &a.vars)?, read(&a.input_file, &a.inputs)?, read(&a.env_file, &a.env)?))
}

/// The secrets of a run and its token.
type Secrets = (Vec<(String, String)>, Option<String>);

fn secrets(a: &Args) -> Result<Secrets, String> {
    use podrunner_local::secrets::{from_env, read_file};
    let mut out: Vec<(String, String)> = Vec::new();
    let mut add = |more: Vec<(String, String)>| {
        for (k, v) in more {
            out.retain(|(o, _)| !o.eq_ignore_ascii_case(&k));
            out.push((k, v));
        }
    };
    if let Some(file) = a.checkout.as_ref().map(|d| d.join(".secrets")).filter(|f| f.is_file()) {
        add(read_file(&file)?);
    }
    if let Some(file) = &a.secret_file {
        add(read_file(file)?);
    }
    for name in &a.secrets {
        add(vec![from_env(name)?]);
    }
    let token = match &a.token_env {
        Some(name) => {
            Some(std::env::var(name).map_err(|_| format!("--github-token-env {name}: the environment has no {name}"))?)
        }
        None => None,
    };
    Ok((out, token))
}

/// Run `podrunner run ARGS`.
pub fn run(args: &[String]) -> Out {
    let a = match parse(args) {
        Ok(a) => a,
        Err(e) => return fail(Class::Usage, format!("{e}\n{USAGE}")),
    };
    let text = match std::fs::read_to_string(&a.file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return fail(Class::NotFound, format!("{}: {e}", a.file.display()));
        }
        Err(e) => return fail(Class::Unavailable, format!("{}: {e}", a.file.display())),
    };
    if a.validate || a.list || a.graph {
        return match pod_actions_workflow::parse(&text) {
            Err(e) => fail(Class::Config, format!("{}: {e}", a.file.display())),
            Ok(p) => {
                let mut stdout = String::new();
                if a.validate {
                    for w in &p.warnings {
                        stdout.push_str(&format!("warning: {w}\n"));
                    }
                    stdout.push_str(&format!("valid: {}\n", a.file.display()));
                }
                if a.list {
                    stdout.push_str(&inspect::list(&p.workflow));
                }
                if a.graph {
                    match inspect::graph(&p.workflow) {
                        Ok(g) => stdout.push_str(&g),
                        Err(e) => return fail(Class::Config, format!("{}: {e}", a.file.display())),
                    }
                }
                Out { code: 0, stdout, stderr: String::new() }
            }
        };
    }
    // A workflow whose `on:` does not take the event does not run on GitHub
    // (T-125); --force-event runs it.
    if !a.force_event
        && let Ok(p) = pod_actions_workflow::parse(&text)
    {
        let events = context::events(&p.workflow.on);
        if !events.contains(&a.event) {
            let msg = format!(
                "{}: `on:` takes {events:?}, not the event {:?}; --force-event runs it",
                a.file.display(),
                a.event
            );
            return fail(Class::Config, msg);
        }
    }
    // The files of vars, inputs and env, then the flags over them (T-126).
    let (vars, inputs, env) = match files(&a) {
        Ok(x) => x,
        Err(e) => return fail(Class::Config, e),
    };
    let mut facts = context::facts(&a);
    let on = pod_actions_workflow::parse(&text).map(|p| p.workflow.on).ok();
    let typed = match (&on, a.event.as_str()) {
        (Some(on), "workflow_dispatch") => match context::dispatch_inputs(on, &inputs) {
            Ok((typed, event)) => {
                facts.inputs = event;
                Value::object(typed)
            }
            Err(e) => return fail(Class::Config, format!("{}: {e}", a.file.display())),
        },
        _ => object(&inputs),
    };
    let github = match context::github(&a, &facts) {
        Ok(g) => g,
        Err(e) => return fail(Class::Config, e),
    };
    let base = Base { github, vars: object(&vars), inputs: typed, ..Base::default() };
    let mut source = match LocalSource::new("local", &text, base, arch(), Box::new(|_, _| Default::default())) {
        Ok(s) => s,
        Err(e) => return fail(Class::Config, format!("{}: {e}", a.file.display())),
    };
    source.set_env(env);
    if let Err(e) = source.set_filter(&a.jobs, &a.matrix) {
        return fail(Class::Usage, e);
    }
    // The values to mask in each line that podrunner writes (T-134).
    let mut masked: Vec<String> = Vec::new();
    match secrets(&a) {
        Ok((list, token)) => {
            masked.extend(list.iter().map(|(_, v)| v.clone()));
            masked.extend(token.clone());
            source.set_secrets(list, token)
        }
        Err(e) => return fail(Class::Config, e),
    }
    let mode = match (a.offline, a.frozen) {
        (true, _) => podrunner_local::store::Mode::Offline,
        (_, true) => podrunner_local::store::Mode::Frozen,
        _ => podrunner_local::store::Mode::Online,
    };
    // The images of a unit kind come into its store by `podvm pull` (T-130).
    let binary = a.podvm.clone().or_else(|| a.guest.as_ref().map(|g| g.1.clone()));
    let pull = match (binary, &a.images) {
        (Some(binary), Some(store)) => Some(podrunner_local::images::PodvmPull {
            binary,
            store: store.clone(),
            platform: if arch() == "aarch64" { "linux/arm64".into() } else { "linux/amd64".into() },
        }),
        _ => None,
    };
    let image_source = pull.as_ref().map(|p| p as &dyn podrunner_local::images::ImageSource);
    let (store, pins, note) =
        match actions::pin(&a.file, &text, a.actions.as_deref(), mode, &facts.repository, image_source) {
            Ok(Some(p)) => (p.dir.or_else(|| a.actions.clone()), p.commits, p.note),
            Ok(None) => (a.actions.clone(), Default::default(), String::new()),
            Err(e) => return fail(Class::Config, format!("{}: the actions: {e}", a.file.display())),
        };
    source.set_pins(pins.clone());
    let scratch = std::env::temp_dir().join(format!("podrunner-run-tree-{}", std::process::id()));
    let tree = match &a.checkout {
        None => None,
        Some(dir) => match podrunner_local::tree::tarball(
            dir,
            &scratch,
            podrunner_local::tree::Options { head: a.checkout_head, with_git: a.with_git },
        ) {
            Ok(t) => Some(t),
            Err(e) => return fail(Class::Config, e),
        },
    };
    let kept = a.artifacts.clone().unwrap_or_else(|| scratch.join("artifacts"));
    let artifacts = match podrunner_local::artifacts::ArtifactStore::new(&kept, podrunner_local::artifacts::LIMIT) {
        Ok(s) => std::sync::Arc::new(s),
        Err(e) => return fail(Class::Config, format!("{}: {e}", kept.display())),
    };
    let cache = match (a.no_cache, a.cache.clone().map(Ok).unwrap_or_else(|| actions::user_cache("cache"))) {
        (true, _) => None,
        (false, Err(e)) => return fail(Class::Config, e),
        (false, Ok(dir)) => match podrunner_local::cache::CacheStore::new(&dir, podrunner_local::cache::LIMIT) {
            // The scope of the ref, then that of the default branch (T-125).
            Ok(s) => Some(std::sync::Arc::new(
                s.with_scopes(vec![facts.git_ref.clone(), format!("refs/heads/{}", facts.default_branch)]),
            )),
            Err(e) => return fail(Class::Config, format!("{}: {e}", dir.display())),
        },
    };
    let mut stdout = String::new();
    let mut stderr = note;
    podrunner_local::cancel::install();
    let (dry, as_json) = (a.dry_run, a.json);
    let mut each = |l: &Line| {
        if as_json {
            return stdout.push_str(&logs::line_json(l));
        }
        let text = line(l);
        // A dry run runs no job: a job that would start is `planned`.
        stdout.push_str(&if dry && text.starts_with("ok ") { text.replacen("ok      ", "planned ", 1) } else { text })
    };
    let state = std::env::temp_dir().join(format!("podrunner-run-units-{}", std::process::id()));
    let common = routes::Common {
        actions: store.clone(),
        pins: pins.clone(),
        artifacts: artifacts.clone(),
        cache: cache.clone(),
        checkout: tree.clone(),
    };
    let built = match a.dry_run {
        true => Ok(Box::new(inspect::DryRun) as Box<dyn podrunner_local::drive::Executor>),
        false => routes::build(&a, &common, &state),
    };
    let mut exec = match built {
        Ok(e) => e,
        Err(o) => {
            let _ = std::fs::remove_dir_all(&state);
            let _ = std::fs::remove_dir_all(&scratch);
            return o;
        }
    };
    if !a.quiet {
        exec.set_live(logs::live(masked.clone(), a.json));
    }
    let result = drive(&mut source, exec.as_mut(), &mut each);
    let logs = exec.take_logs();
    let blobs = exec.take_blobs();
    if let Some(dir) = &a.log_dir
        && let Err(e) = logs::write_dir(dir, &blobs, &masked)
    {
        stderr.push_str(&format!("podrunner: --log-dir: {e}\n"));
    }
    drop(exec);
    let _ = std::fs::remove_dir_all(&state);
    let _ = std::fs::remove_dir_all(kept.join(".pending"));
    let _ = std::fs::remove_dir_all(&scratch);
    // Each request with no route of the fake broker (T-145), named on
    // stderr whatever the result: it can explain a failed step.
    let missing: Vec<String> = logs
        .iter()
        .flat_map(|(job, log)| {
            log.lines()
                .filter_map(|l| l.strip_prefix(podrunner_local::exec::NO_ROUTE))
                .map(move |m| format!("podrunner: {job}: no route of the fake broker: {m}\n"))
        })
        .collect();
    if !missing.is_empty() {
        stderr.extend(missing.iter().map(String::as_str));
        stderr.push_str(&format!("podrunner: {} requests had no route of the fake broker\n", missing.len()));
    }
    if podrunner_local::cancel::requested() {
        stderr.push_str("podrunner: a signal cancelled the run\n");
        return Out { code: 130, stdout, stderr };
    }
    match result {
        Ok(true) => Out { code: 0, stdout, stderr },
        Ok(false) => {
            for (job, log) in &logs {
                stderr.push_str(&format!("--- log of {job}\n{log}\n"));
            }
            Out { code: 1, stdout, stderr }
        }
        Err(e) => {
            stderr.push_str(&format!("podrunner: {e}\n"));
            Out { code: Class::Protocol.exit_code(), stdout, stderr }
        }
    }
}

#[cfg(test)]
mod tests;
