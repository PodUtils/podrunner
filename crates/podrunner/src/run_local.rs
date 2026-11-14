//! `podrunner run FILE` (T-089, the local mode of T-046): run the jobs of
//! one workflow file, with no forge. The local source (`podrunner-local`)
//! orders and expands the jobs; the official runner runs each one against
//! a fake broker on the loopback: in DIR with `--runner-dir`, or in a fresh
//! podvm unit with `--podvm` (T-090). One line for each job; the exit code is 0 when no job failed,
//! 1 when one failed, and the code of the class of a usage or a file error.

mod actions;
mod args;
mod concurrency;
mod context;
mod doctor;
mod guest;
mod inspect;
mod logs;
mod many;
mod pause;
mod protect;
mod routes;
mod slots;
mod watch;
mod web;
pub use watch::repeat;

use std::path::PathBuf;
use std::sync::Arc;

use pod_actions_expr::Value;
use pod_actions_workflow::{Base, JobResult};
use pod_core::failure::Class;
use podrunner_local::LocalSource;
use podrunner_local::drive::Line;

use crate::cli::Out;

use args::{Args, parse};

pub const USAGE: &str = "usage: podrunner run (FILE | DIR) (--runner-dir DIR | (--podvm PATH | --microvm PATH [--engine DIR] | --incus PATH) --images STORE --image NAME@sha256:HEX)\n         [--podnet PATH] [--allow HOST]... [--platform LABEL=NAME@sha256:HEX|host]... [--actions DIR] [--offline | --frozen] [--artifacts DIR] [--cache DIR | --no-cache] [--secret NAME]... [--secret-file FILE] [--github-token-env NAME] [--job ID]... [--matrix KEY=VALUE]... [--list | --graph | --validate | --dry-run] [-q] [--json] [--log-dir DIR] [--environment-secret-file NAME=FILE]... [--environment-var-file NAME=FILE]... [--environment-allow NAME=HOST]... [--protected NAME]... [--approve NAME]... [--export DIR] [--toolcache DIR] [--local-repository OWNER/REPO@REF=DIR]... [--action-server URL] [--oidc-issuer URL] [--oidc-keys FILE] [--watch | --schedule] [--no-config] [--changed-since REF] [--web IP:PORT] [--jobs N] [--break JOB:STEP]... [--break-on-failure | --keep-failed] [--checkout DIR [--checkout-head] [--with-git]] [--event NAME] [--event-path FILE] [--force-event] [--remote NAME] [--ref REF] [--sha SHA] [--repository OWNER/REPO] [--actor NAME] [--var NAME=VALUE]... [--input NAME=VALUE]... [--var-file FILE] [--input-file FILE] [--env NAME=VALUE]... [--env-file FILE] [--state DIR] [--rerun DIR [--failed | --job ID]... [--refresh]]";

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

/// The line of one job.
pub fn line(l: &Line) -> String {
    match l {
        Line::Ran { name, result: JobResult::Success, .. } => format!("ok      {name}\n"),
        Line::Ran { name, result, .. } => format!("{:<7} {name}\n", result.as_str()),
        Line::Refused { name, reason, .. } => format!("failure {name}: {reason}\n"),
        Line::Skipped { job, condition } => format!("skipped {job}: if: {condition}\n"),
        Line::NotSelected { name, reason, .. } => format!("skipped {name}: {reason}\n"),
        Line::Kept { name, result, .. } => format!("kept    {name} ({})\n", result.as_str()),
    }
}

/// The secrets of the run (T-121) and the token of `github.token` (T-122):
/// `.secrets` of the checkout when it exists, then `--secret-file`, then
/// each `--secret`.
/// The secrets and vars of each environment (T-136): a secret file needs
/// mode 0600, as `--secret-file`.
fn environments(a: &Args) -> Result<std::collections::BTreeMap<String, podrunner_local::Environment>, String> {
    let mut out: std::collections::BTreeMap<String, podrunner_local::Environment> = Default::default();
    for (name, file) in &a.environment_secrets {
        out.entry(name.clone()).or_default().secrets.extend(podrunner_local::secrets::read_file(file)?);
    }
    for (name, file) in &a.environment_vars {
        let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let vars = podrunner_local::secrets::parse_pairs(&text).map_err(|e| format!("{}: {e}", file.display()))?;
        out.entry(name.clone()).or_default().vars.extend(vars);
    }
    Ok(out)
}

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

/// `podrunner doctor --run FILE [the options of run]` (T-153): the checks of
/// the inputs of a run; exit 0 when each holds, 1 when one fails.
pub fn doctor(args: &[String]) -> Out {
    let a = match parse(args) {
        Ok(a) => a,
        Err(e) => return fail(Class::Usage, format!("{e}\n{USAGE}")),
    };
    let checks = doctor::checks(&a);
    let code = if checks.iter().all(|c| c.ok) { 0 } else { 1 };
    Out { code, stdout: doctor::text(&checks), stderr: String::new() }
}

/// Run `podrunner run ARGS`.
pub fn run(args: &[String]) -> Out {
    let a = match parse(args) {
        Ok(a) => a,
        Err(e) => return fail(Class::Usage, format!("{e}\n{USAGE}")),
    };
    // A directory: each of its workflows that takes the event (T-125).
    if a.file.is_dir() {
        return many::run_dir(args, &a);
    }
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
    // The files of vars, inputs and env, then the flags over them (T-126).
    let (vars, inputs, env) = match files(&a) {
        Ok(x) => x,
        Err(e) => return fail(Class::Config, e),
    };
    let mut facts = context::facts(&a);
    let parsed = pod_actions_workflow::parse(&text).ok();
    // A workflow whose `on:` does not take the event after its filters does
    // not run on GitHub (T-125); --force-event runs it.
    if !a.force_event
        && let Some(p) = &parsed
    {
        match context::takes(&a, &facts, &p.workflow) {
            Ok(true) => {}
            Ok(false) => {
                let events = context::events(&p.workflow.on);
                let msg = format!(
                    "{}: `on:` ({events:?}) does not take the event {:?} of {} after its filters; --force-event runs it",
                    a.file.display(),
                    a.event,
                    facts.git_ref
                );
                return fail(Class::Config, msg);
            }
            Err(e) => return fail(Class::Config, format!("{}: {e}", a.file.display())),
        }
    }
    let on = parsed.map(|p| p.workflow.on);
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
    let (store, pins, note) = match actions::pin(&a, &text, mode, &facts.repository, image_source) {
        Ok(Some(p)) => (p.dir.or_else(|| a.actions.clone()), p.commits, p.note),
        Ok(None) => (a.actions.clone(), Default::default(), String::new()),
        Err(e) => return fail(Class::Config, format!("{}: the actions: {e}", a.file.display())),
    };
    let base = Base { github, vars: object(&vars), inputs: typed, ..Base::default() };
    // The texts of the called workflows: the tree, and the store (T-131).
    let root = a.checkout.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));
    let texts = Arc::new(podrunner_local::calls::Texts::new(root, store.clone(), pins.clone()));
    let minter = Box::new(|_: &_, _: &_| Default::default());
    let mut source = match LocalSource::new_with("local", &text, base, arch(), minter, Some(texts)) {
        Ok(s) => s,
        Err(e) => return fail(Class::Config, format!("{}: {e}", a.file.display())),
    };
    source.set_env(env);
    // A rerun (T-135): the kept end of each cell that it does not run; its
    // `--job` names the jobs to run again, not a filter.
    let state_dir = a.state.clone().or_else(|| a.rerun.clone());
    let digest = podrunner_local::store::digest(text.as_bytes());
    if let Some(dir) = &a.rerun {
        let kept = std::fs::read_to_string(dir.join("state.json"))
            .map_err(|e| format!("{}: {e}", dir.join("state.json").display()))
            .and_then(|t| podrunner_local::rerun::parse_state(&t))
            .and_then(|(was, ends)| {
                if was != digest && !a.refresh {
                    return Err(format!(
                        "{}: the workflow changed since the run of {}; --refresh reruns it",
                        a.file.display(),
                        dir.display()
                    ));
                }
                let set = podrunner_local::rerun::rerun_jobs(source.workflow(), &ends, &a.jobs)?;
                Ok(podrunner_local::rerun::kept(&ends, &set))
            });
        match kept {
            Ok(k) => source.set_kept(k),
            Err(e) => return fail(Class::Config, e),
        }
    }
    let filter_jobs: &[String] = if a.rerun.is_some() { &[] } else { &a.jobs };
    if let Err(e) = source.set_filter(filter_jobs, &a.matrix) {
        return fail(Class::Usage, e);
    }
    // The values to mask in each line that podrunner writes (T-134).
    let mut masked: Vec<String> = Vec::new();
    // The secrets and vars of each environment (T-136).
    match environments(&a) {
        Ok(mut envs) => {
            // Only a run of jobs asks for an approval (T-136).
            if !a.dry_run {
                protect::hold(&a, source.workflow(), &mut envs, &mut protect::terminal);
            }
            masked.extend(envs.values().flat_map(|e| e.secrets.iter().map(|(_, v)| v.clone())));
            source.set_environments(envs);
        }
        Err(e) => return fail(Class::Config, e),
    }
    match secrets(&a) {
        Ok((list, token)) => {
            masked.extend(list.iter().map(|(_, v)| v.clone()));
            masked.extend(token.clone());
            source.set_secrets(list, token)
        }
        Err(e) => return fail(Class::Config, e),
    }
    source.set_pins(guest::mirror_pins(&a, &pins, source.workflow()));
    let scratch = std::env::temp_dir().join(format!("podrunner-run-tree-{}", std::process::id()));
    let tree = match &a.checkout {
        None => None,
        Some(dir) => match podrunner_local::tree::tarball(
            dir,
            &scratch,
            podrunner_local::tree::Options { head: a.checkout_head, with_git: a.with_git },
        ) {
            Ok(t) => {
                // Each checkout of each job downloads the tree: a guest takes
                // it through its channel, which is slow for a large tree.
                let mib = std::fs::metadata(&t).map(|m| m.len() >> 20).unwrap_or(0);
                if mib >= 100 {
                    eprintln!(
                        "podrunner: the tree of --checkout is {mib} MiB; each checkout of a job downloads it: ignore its large files in .gitignore, or give --checkout-head"
                    );
                }
                Some(t)
            }
            Err(e) => return fail(Class::Config, e),
        },
    };
    // The artifacts stay in the state of the run, so that a rerun job reads
    // the artifact of a kept job; a rerun replaces what it uploads again.
    let kept = a
        .artifacts
        .clone()
        .or_else(|| state_dir.as_ref().map(|d| d.join("artifacts")))
        .unwrap_or_else(|| scratch.join("artifacts"));
    let artifacts = match podrunner_local::artifacts::ArtifactStore::new(&kept, podrunner_local::artifacts::LIMIT) {
        Ok(s) if a.rerun.is_some() => std::sync::Arc::new(s.replacing()),
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
    // The pauses wait for Enter on the terminal (T-150); a dry run has none.
    if !a.breaks.is_empty()
        && !a.dry_run
        && let Err(e) = pause::watch()
    {
        let _ = std::fs::remove_dir_all(&scratch);
        return fail(Class::Usage, e);
    }
    let web = match a.web.map(|addr| web::Web::start(addr, artifacts.clone())) {
        None => None,
        Some(Ok(w)) => {
            eprintln!("podrunner: the web view: {}", w.url);
            Some(w)
        }
        Some(Err(e)) => {
            let _ = std::fs::remove_dir_all(&scratch);
            return fail(Class::Config, e);
        }
    };
    let mut each = |l: &Line| {
        if let Some(w) = &web {
            w.line(l);
        }
        if as_json {
            return stdout.push_str(&logs::line_json(l));
        }
        let text = line(l);
        // A dry run runs no job: a job that would start is `planned`.
        stdout.push_str(&if dry && text.starts_with("ok ") { text.replacen("ok      ", "planned ", 1) } else { text })
    };
    let state = std::env::temp_dir().join(format!("podrunner-run-units-{}", std::process::id()));
    let oidc = match routes::issuer(&a) {
        Ok(i) => i,
        Err(e) => return fail(Class::Config, e),
    };
    let common = routes::Common {
        actions: store.clone(),
        pins: pins.clone(),
        artifacts: artifacts.clone(),
        cache: cache.clone(),
        checkout: tree.clone(),
        oidc,
    };
    let to_stderr = (!a.quiet).then(|| logs::live(masked.clone(), a.json));
    let live = match (&web, to_stderr) {
        (Some(w), then) => Some(w.live(masked.clone(), then)),
        (None, l) => l,
    };
    let notes_sink: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    // The groups of the run and of its jobs, held across runs (T-137); a dry
    // run holds none.
    let mut group_notes: Vec<String> = Vec::new();
    let groups = actions::user_cache("concurrency");
    let run_group = match (a.dry_run, source.run_group(), &groups) {
        (true, _, _) => Ok(None),
        (_, Err(e), _) => Err(e),
        (_, Ok(None), _) => Ok(None),
        (_, Ok(Some(g)), Ok(dir)) => {
            concurrency::hold(dir, &facts.repository, &g, &mut |n| group_notes.push(n)).map(Some)
        }
        (_, Ok(Some(_)), Err(e)) => Err(e.clone()),
    };
    let job_groups = match (a.dry_run, groups) {
        (false, Ok(dir)) => Some((dir, facts.repository.clone(), Arc::clone(&notes_sink))),
        _ => None,
    };
    let built = run_group
        .map_err(|e| fail(Class::Config, e))
        .and_then(|held| slots::build(&a, &common, &state, live, job_groups).map(|s| (held, s)));
    let (_run_held, execs) = match built {
        Ok(b) => b,
        Err(o) => {
            let _ = std::fs::remove_dir_all(&state);
            let _ = std::fs::remove_dir_all(&scratch);
            return o;
        }
    };
    let (result, (logs, blobs)) = slots::run(&mut source, execs, &mut each);
    group_notes.extend(notes_sink.lock().map(|n| n.clone()).unwrap_or_default());
    stderr.extend(group_notes.iter().map(String::as_str));
    let log_dir = a.log_dir.clone().or_else(|| state_dir.as_ref().map(|d| d.join("logs")));
    if let Some(dir) = &log_dir
        && let Err(e) = logs::write_dir(dir, &blobs, &masked)
    {
        stderr.push_str(&format!("podrunner: --log-dir: {e}\n"));
    }
    // The state of the run, for a rerun (T-135).
    if let Some(dir) = &state_dir {
        let text = podrunner_local::rerun::state_json(&digest, source.ends());
        if let Err(e) = std::fs::create_dir_all(dir).and_then(|()| std::fs::write(dir.join("state.json"), text)) {
            stderr.push_str(&format!("podrunner: --state {}: {e}\n", dir.display()));
        }
    }
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
    let out = match result {
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
    };
    web::hold(web.as_ref(), out)
}

#[cfg(test)]
mod tests;
