//! `podrunner run FILE` (T-089, the local mode of T-046): run the jobs of
//! one workflow file, with no forge. The local source (`podrunner-local`)
//! orders and expands the jobs; the official runner runs each one against
//! a fake broker on the loopback: in DIR with `--runner-dir`, or in a fresh
//! podvm unit with `--podvm` (T-090). One line for each job; the exit code is 0 when no job failed,
//! 1 when one failed, and the code of the class of a usage or a file error.

mod actions;
mod context;
mod guest;
mod routes;

use std::path::PathBuf;

use pod_actions_expr::Value;
use pod_actions_workflow::{Base, JobResult};
use pod_core::failure::Class;
use podrunner_local::LocalSource;
use podrunner_local::drive::{Line, drive};
use podrunner_local::exec::InUnit;

use crate::cli::Out;

pub const USAGE: &str = "usage: podrunner run FILE (--runner-dir DIR | (--podvm PATH | --microvm PATH [--engine DIR] | --incus PATH) --images STORE --image NAME@sha256:HEX)\n         [--podnet PATH] [--allow HOST]... [--platform LABEL=NAME@sha256:HEX|host]... [--actions DIR] [--offline | --frozen] [--artifacts DIR] [--cache DIR | --no-cache] [--secret NAME]... [--secret-file FILE] [--github-token-env NAME] [--checkout DIR [--checkout-head] [--with-git]] [--event NAME] [--event-path FILE] [--force-event] [--remote NAME] [--ref REF] [--sha SHA] [--repository OWNER/REPO] [--actor NAME] [--var NAME=VALUE]... [--input NAME=VALUE]...";

fn fail(class: Class, msg: impl Into<String>) -> Out {
    Out { code: class.exit_code(), stdout: String::new(), stderr: format!("podrunner: {}\n", msg.into()) }
}

/// The options of `run`.
#[derive(Debug, Default, PartialEq, Clone)]
struct Args {
    file: PathBuf,
    runner_dir: Option<PathBuf>,
    podvm: Option<PathBuf>,
    /// `--microvm PATH` (true) or `--incus PATH` (false): a guest kind of
    /// the podvm binary PATH (T-123).
    guest: Option<(bool, PathBuf)>,
    engine: Option<PathBuf>,
    /// `--platform LABEL=TARGET`: a runner image, or `host`, for a label of
    /// `runs-on` (T-129).
    platforms: Vec<(String, String)>,
    images: Option<PathBuf>,
    image: Option<String>,
    actions: Option<PathBuf>,
    offline: bool,
    frozen: bool,
    artifacts: Option<PathBuf>,
    cache: Option<PathBuf>,
    no_cache: bool,
    secrets: Vec<String>,
    secret_file: Option<PathBuf>,
    token_env: Option<String>,
    checkout: Option<PathBuf>,
    checkout_head: bool,
    with_git: bool,
    podnet: Option<PathBuf>,
    allow: Vec<String>,
    event: String,
    event_path: Option<PathBuf>,
    force_event: bool,
    remote: Option<String>,
    git_ref: Option<String>,
    sha: Option<String>,
    repository: Option<String>,
    actor: Option<String>,
    vars: Vec<(String, String)>,
    inputs: Vec<(String, String)>,
}

fn pair(v: &str) -> Result<(String, String), String> {
    let (k, v) = v.split_once('=').ok_or_else(|| format!("{v:?} is not NAME=VALUE"))?;
    if !pod_actions_expr::is_name(k) {
        return Err(format!("{k:?} is not a name"));
    }
    Ok((k.to_string(), v.to_string()))
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut a = Args { event: "push".into(), ..Args::default() };
    let mut it = args.iter();
    let mut file = None;
    while let Some(x) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("{x} needs a value"));
        match x.as_str() {
            "--runner-dir" => a.runner_dir = Some(PathBuf::from(value()?)),
            "--podvm" => a.podvm = Some(PathBuf::from(value()?)),
            "--microvm" => a.guest = Some((true, PathBuf::from(value()?))),
            "--incus" => a.guest = Some((false, PathBuf::from(value()?))),
            "--engine" => a.engine = Some(PathBuf::from(value()?)),
            "--platform" => {
                let v = value()?;
                let (label, target) =
                    v.split_once('=').ok_or_else(|| format!("--platform {v:?} is not LABEL=TARGET"))?;
                if label.is_empty() || label.contains(char::is_whitespace) {
                    return Err(format!("--platform {v:?}: not a label"));
                }
                if target != "host" {
                    podrunner_local::message::pinned(target).map_err(|e| format!("--platform {v:?}: {e}"))?;
                }
                a.platforms.push((label.to_string(), target.to_string()));
            }
            "--images" => a.images = Some(PathBuf::from(value()?)),
            "--image" => a.image = Some(value()?),
            "--actions" => a.actions = Some(PathBuf::from(value()?)),
            "--artifacts" => a.artifacts = Some(PathBuf::from(value()?)),
            "--secret" => {
                let name = value()?;
                if name.contains('=') {
                    return Err("--secret takes a NAME of the environment, never a value in argv".into());
                }
                a.secrets.push(name);
            }
            "--secret-file" => a.secret_file = Some(PathBuf::from(value()?)),
            "--github-token-env" => a.token_env = Some(value()?),
            "--cache" => a.cache = Some(PathBuf::from(value()?)),
            "--no-cache" => a.no_cache = true,
            "--offline" => a.offline = true,
            "--frozen" => a.frozen = true,
            "--checkout" => a.checkout = Some(PathBuf::from(value()?)),
            "--checkout-head" => a.checkout_head = true,
            "--with-git" => a.with_git = true,
            "--podnet" => a.podnet = Some(PathBuf::from(value()?)),
            "--allow" => a.allow.push(value()?),
            "--event" => a.event = value()?,
            "--event-path" => a.event_path = Some(PathBuf::from(value()?)),
            "--force-event" => a.force_event = true,
            "--remote" => a.remote = Some(value()?),
            "--ref" => a.git_ref = Some(value()?),
            "--sha" => a.sha = Some(value()?),
            "--repository" => a.repository = Some(value()?),
            "--actor" => a.actor = Some(value()?),
            "--var" => a.vars.push(pair(&value()?)?),
            "--input" => a.inputs.push(pair(&value()?)?),
            o if o.starts_with("--") => return Err(format!("an unknown option {o:?}")),
            f if file.is_none() => file = Some(PathBuf::from(f)),
            f => return Err(format!("a second file {f:?}")),
        }
    }
    a.file = file.ok_or("the workflow file is necessary")?;
    if a.cache.is_some() && a.no_cache {
        return Err("--cache and --no-cache exclude each other".into());
    }
    if (a.checkout_head || a.with_git) && a.checkout.is_none() {
        return Err("--checkout-head and --with-git need --checkout".into());
    }
    if a.offline && a.frozen {
        return Err("--offline and --frozen exclude each other".into());
    }
    if a.podvm.is_some() && a.guest.is_some() {
        return Err("one of --podvm, --microvm and --incus".into());
    }
    let kind = a.podvm.is_some() || a.guest.is_some();
    if !a.platforms.is_empty() {
        let host = a.platforms.iter().any(|(_, t)| t == "host");
        let images = a.platforms.iter().any(|(_, t)| t != "host");
        if host && a.runner_dir.is_none() {
            return Err("--platform LABEL=host needs --runner-dir DIR".into());
        }
        if images && (!kind || a.images.is_none()) {
            return Err("--platform LABEL=IMAGE needs a unit kind (--podvm, --microvm, --incus) and --images".into());
        }
        if a.image.is_some() && !kind {
            return Err("--image needs a unit kind".into());
        }
        return Ok(a);
    }
    let unit = [kind, a.images.is_some(), a.image.is_some()];
    if a.podnet.is_some() && a.podvm.is_none() {
        return Err("--podnet needs --podvm".into());
    }
    // A guest has no NIC: the egress module holds --allow with no podnet.
    if !a.allow.is_empty() && a.podnet.is_none() && a.guest.is_none() {
        return Err("--allow needs --podnet with --podvm, or --microvm or --incus".into());
    }
    if a.engine.is_some() && !matches!(a.guest, Some((true, _))) {
        return Err("--engine needs --microvm".into());
    }
    match (a.runner_dir.is_some(), unit) {
        (true, [false, false, false]) | (false, [true, true, true]) => Ok(a),
        _ => Err("one of --runner-dir DIR, or a unit kind (--podvm, --microvm, --incus) with --images and --image, is necessary".into()),
    }
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
    }
}

/// The secrets of the run (T-121) and the token of `github.token` (T-122):
/// `.secrets` of the checkout when it exists, then `--secret-file`, then
/// each `--secret`.
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
    let facts = context::facts(&a);
    let github = match context::github(&a, &facts) {
        Ok(g) => g,
        Err(e) => return fail(Class::Config, e),
    };
    let base = Base { github, vars: object(&a.vars), inputs: object(&a.inputs), ..Base::default() };
    let mut source = match LocalSource::new("local", &text, base, arch(), Box::new(|_, _| Default::default())) {
        Ok(s) => s,
        Err(e) => return fail(Class::Config, format!("{}: {e}", a.file.display())),
    };
    match secrets(&a) {
        Ok((list, token)) => source.set_secrets(list, token),
        Err(e) => return fail(Class::Config, e),
    }
    let mode = match (a.offline, a.frozen) {
        (true, _) => podrunner_local::store::Mode::Offline,
        (_, true) => podrunner_local::store::Mode::Frozen,
        _ => podrunner_local::store::Mode::Online,
    };
    let (store, pins, note) = match actions::pin(&a.file, &text, a.actions.as_deref(), mode, &facts.repository) {
        Ok(Some(p)) => (Some(p.dir), p.commits, p.note),
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
    let mut each = |l: &Line| stdout.push_str(&line(l));
    let state = std::env::temp_dir().join(format!("podrunner-run-units-{}", std::process::id()));
    let common = routes::Common {
        actions: store.clone(),
        pins: pins.clone(),
        artifacts: artifacts.clone(),
        cache: cache.clone(),
        checkout: tree.clone(),
    };
    let mut exec = match routes::build(&a, &common, &state) {
        Ok(e) => e,
        Err(o) => {
            let _ = std::fs::remove_dir_all(&state);
            let _ = std::fs::remove_dir_all(&scratch);
            return o;
        }
    };
    let result = drive(&mut source, exec.as_mut(), &mut each);
    let logs = exec.take_logs();
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
