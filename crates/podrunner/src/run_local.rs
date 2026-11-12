//! `podrunner run FILE --runner-dir DIR` (T-089, the local mode of T-046):
//! run the jobs of one workflow file on this host, with no forge. The
//! local source (`podrunner-local`) orders and expands the jobs; the
//! official runner in DIR runs each one against a fake broker on the
//! loopback. One line for each job; the exit code is 0 when no job failed,
//! 1 when one failed, and the code of the class of a usage or a file error.

use std::path::PathBuf;

use pod_actions_expr::Value;
use pod_actions_workflow::{Base, JobResult};
use pod_core::failure::Class;
use podrunner_local::LocalSource;
use podrunner_local::drive::{Line, drive};
use podrunner_local::exec::{InUnit, RunnerDir};

use crate::cli::Out;

pub const USAGE: &str = "usage: podrunner run FILE (--runner-dir DIR | --podvm PATH --images STORE --image NAME@sha256:HEX)\n         [--actions DIR] [--event NAME] [--var NAME=VALUE]... [--input NAME=VALUE]...";

fn fail(class: Class, msg: impl Into<String>) -> Out {
    Out { code: class.exit_code(), stdout: String::new(), stderr: format!("podrunner: {}\n", msg.into()) }
}

/// The options of `run`.
#[derive(Debug, Default, PartialEq)]
struct Args {
    file: PathBuf,
    runner_dir: Option<PathBuf>,
    podvm: Option<PathBuf>,
    images: Option<PathBuf>,
    image: Option<String>,
    actions: Option<PathBuf>,
    event: String,
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
            "--images" => a.images = Some(PathBuf::from(value()?)),
            "--image" => a.image = Some(value()?),
            "--actions" => a.actions = Some(PathBuf::from(value()?)),
            "--event" => a.event = value()?,
            "--var" => a.vars.push(pair(&value()?)?),
            "--input" => a.inputs.push(pair(&value()?)?),
            o if o.starts_with("--") => return Err(format!("an unknown option {o:?}")),
            f if file.is_none() => file = Some(PathBuf::from(f)),
            f => return Err(format!("a second file {f:?}")),
        }
    }
    a.file = file.ok_or("the workflow file is necessary")?;
    let unit = [a.podvm.is_some(), a.images.is_some(), a.image.is_some()];
    match (a.runner_dir.is_some(), unit) {
        (true, [false, false, false]) | (false, [true, true, true]) => Ok(a),
        _ => Err("one of --runner-dir DIR, or --podvm, --images and --image together, is necessary".into()),
    }
}

fn object(pairs: &[(String, String)]) -> Value {
    Value::object(pairs.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))))
}

/// The local stand-in of the `github` context of the job level.
fn github(file: &std::path::Path, event: &str) -> Value {
    let workflow = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Value::object([
        ("event_name", Value::String(event.into())),
        ("workflow", Value::String(workflow)),
        ("repository", Value::String("local/workflow".into())),
        ("server_url", Value::String("https://github.com".into())),
        ("run_id", Value::String("1".into())),
        ("run_number", Value::String("1".into())),
    ])
}

fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" }
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
    let unit = PodvmUnit::new(podvm, state, images).map_err(|e| fail(Class::Config, format!("podvm: {e}")))?;
    let platform = if arch() == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let prepared = unit.prepare(image, platform, false).map_err(|e| fail(Class::Config, format!("{image}: {e}")))?;
    Ok(InUnit::new(std::sync::Arc::new(unit), prepared))
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
    let base =
        Base { github: github(&a.file, &a.event), vars: object(&a.vars), inputs: object(&a.inputs), ..Base::default() };
    let mut source = match LocalSource::new("local", &text, base, arch(), Box::new(|_, _| Default::default())) {
        Ok(s) => s,
        Err(e) => return fail(Class::Config, format!("{}: {e}", a.file.display())),
    };
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut each = |l: &Line| stdout.push_str(&line(l));
    let (result, logs) = if let Some(dir) = a.runner_dir.clone() {
        let mut exec = match RunnerDir::new(dir) {
            Ok(x) => x,
            Err(e) => return fail(Class::Config, e),
        };
        exec.actions = a.actions.clone();
        (drive(&mut source, &mut exec, &mut each), exec.logs)
    } else {
        let state = std::env::temp_dir().join(format!("podrunner-run-{}", std::process::id()));
        let mut exec = match in_podvm(&a, &state) {
            Ok(x) => x,
            Err(o) => return o,
        };
        exec.actions = a.actions.clone();
        let r = drive(&mut source, &mut exec, &mut each);
        let _ = std::fs::remove_dir_all(&state);
        (r, exec.logs)
    };
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
