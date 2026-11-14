//! A cell of a workflow job into the job of the fake broker (T-088). GitHub
//! sends each `${{ }}` of a step, of `env` and of `outputs` as a token that
//! the runner evaluates in its own contexts (`steps`, `env`, `runner`,
//! `job`): a text that is one expression is a basic expression, and a text
//! with expressions around literal text is `format()`. The contexts of the
//! job level that the runner cannot know (`matrix`, `strategy`, `needs`,
//! `vars`, `inputs`) go into `contextData`.

use std::collections::BTreeMap;

use pod_actions_expr::{Template, TemplatePart, Value as XValue};
use pod_actions_workflow::{Job as WorkflowJob, Kind, Node, Step as WorkflowStep};
use podrunner_broker_fake::Job;
use podrunner_broker_fake::message::step_condition;
use serde_json::{Map, Value, json};

use crate::Cell;

/// A text of the workflow as a template token.
pub fn token(text: &str) -> Result<Value, String> {
    let template = Template::parse(text).map_err(|e| format!("{text:?}: {e}"))?;
    if template.is_literal() {
        return Ok(json!(text));
    }
    if let Some(e) = template.as_expression() {
        return Ok(json!({ "type": 3, "expr": e.text().trim() }));
    }
    let mut format = String::new();
    let mut args = String::new();
    let mut n = 0;
    for p in template.parts() {
        match p {
            TemplatePart::Text(t) => format.push_str(&t.replace('\'', "''").replace('{', "{{").replace('}', "}}")),
            TemplatePart::Expr(e, _) => {
                format.push_str(&format!("{{{n}}}"));
                args.push_str(&format!(", ({})", e.text().trim()));
                n += 1;
            }
        }
    }
    Ok(json!({ "type": 3, "expr": format!("format('{format}'{args})") }))
}

/// A scalar of the file as a token: a string as a template, a number or a
/// boolean as itself.
fn scalar(node: &Node) -> Result<Value, String> {
    match &node.kind {
        Kind::String(s) => token(s),
        Kind::Bool(b) => Ok(json!(b)),
        Kind::Number(n) => Ok(json!(n)),
        Kind::Null => Ok(json!("")),
        _ => Err(format!("line {}: a scalar is necessary", node.mark.line)),
    }
}

/// An image of a unit must be pinned by digest: the unit takes only a
/// pinned image (`payload-not-pinned`), and a tag can move.
fn pinned(image: &str) -> Result<(), String> {
    match image.split_once("@sha256:") {
        Some((name, hex)) if !name.is_empty() && hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()) => {
            Ok(())
        }
        _ => Err(format!("the image {image:?} is not pinned by digest (NAME@sha256:HEX)")),
    }
}

/// A node of the file as a template token, its strings as templates.
fn node_token(node: &Node) -> Result<Value, String> {
    match &node.kind {
        Kind::Map(entries) => Ok(mapping(
            entries.iter().map(|(k, v)| Ok((k.name.clone(), node_token(v)?))).collect::<Result<_, String>>()?,
        )),
        Kind::Seq(items) => {
            Ok(json!({ "type": 1, "seq": items.iter().map(node_token).collect::<Result<Vec<_>, _>>()? }))
        }
        _ => scalar(node),
    }
}

/// The image of a `container:` or of a service: a string, or `image:` of a
/// mapping; it must be pinned.
fn container_image(node: &Node, what: &str) -> Result<(), String> {
    let image = match &node.kind {
        Kind::String(s) => Some(s.as_str()),
        Kind::Map(e) => e.iter().find(|(k, _)| k.name == "image").and_then(|(_, v)| match &v.kind {
            Kind::String(s) => Some(s.as_str()),
            _ => None,
        }),
        _ => None,
    };
    let image = image.ok_or_else(|| format!("line {}: {what} has no image", node.mark.line))?;
    pinned(image).map_err(|e| format!("line {}: {what}: {e}", node.mark.line))
}

/// A mapping token (`TemplateToken` type 2).
fn mapping(entries: Vec<(String, Value)>) -> Value {
    let map: Vec<Value> = entries.into_iter().map(|(k, v)| json!({ "Key": k, "Value": v })).collect();
    json!({ "type": 2, "map": map })
}

/// An `env:` mapping into its entries as tokens.
fn env(node: Option<&Node>) -> Result<Vec<(String, Value)>, String> {
    let Some(node) = node else { return Ok(Vec::new()) };
    match &node.kind {
        Kind::Map(entries) => entries.iter().map(|(k, v)| Ok((k.name.clone(), scalar(v)?))).collect(),
        Kind::String(_) => Err(format!("line {}: an env of one expression is not supported", node.mark.line)),
        _ => Err(format!("line {}: env must be a mapping", node.mark.line)),
    }
}

/// A value of the contexts into the context data of the runner
/// (`PipelineContextData`): a primitive as itself, an array as type 1, an
/// object as type 2.
pub fn context_data(v: &XValue) -> Value {
    match v {
        XValue::Null => Value::Null,
        XValue::Bool(b) => json!(b),
        XValue::Number(n) => json!(n),
        XValue::String(s) => json!(s),
        XValue::Array(items) => json!({ "t": 1, "a": items.iter().map(context_data).collect::<Vec<_>>() }),
        XValue::Object(o) => {
            let d: Vec<Value> = o.iter().map(|(k, v)| json!({ "k": k, "v": context_data(v) })).collect();
            json!({ "t": 2, "d": d })
        }
    }
}

fn guid(n: u64) -> String {
    format!("00000000-0000-0000-0000-{n:012x}")
}

/// The shell and the directory of `defaults.run`: the job's over the
/// workflow's.
struct RunDefaults<'a> {
    shell: Option<&'a str>,
    working_directory: Option<&'a str>,
}

/// One step as the runner reads it: a script action.
/// The stand-in of `actions/checkout` in the local mode (T-092): the
/// tarball of the local tree from the fake broker (`github.api_url` names
/// it) into the workspace, or into `path` in it. No `tar -C` and no
/// `tar -z`: in a podvm unit GNU `tar -C DIR` failed to open an existing
/// DIR (T-093).
fn checkout_script(path: &str) -> String {
    let dest = if path.is_empty() { "$GITHUB_WORKSPACE".to_string() } else { format!("$GITHUB_WORKSPACE/{path}") };
    format!(
        "set -eu\nmkdir -p \"{dest}\"\ncd \"{dest}\"\ncurl -fsSL \"${{{{ github.api_url }}}}{}\" | gzip -dc | tar -xf -",
        podrunner_broker_fake::actions::CHECKOUT
    )
}

/// The reference of `uses:`, its base context name and its display name.
/// The ref of an action of a repository is a tag, a branch or a commit:
/// `podrunner run` pins each one before the first job (T-118), and the
/// launch route of the fake broker answers from those pins.
fn action(uses: &str, line: u32) -> Result<(Value, String), String> {
    if let Some(image) = uses.strip_prefix("docker://") {
        pinned(image).map_err(|e| format!("line {line}: `uses: {uses}`: {e}"))?;
        return Ok((json!({ "type": "containerRegistry", "image": image }), "__docker".into()));
    }
    if let Some(path) = uses.strip_prefix("./") {
        let reference = json!({ "type": "repository", "repositoryType": "self", "path": format!("./{path}") });
        return Ok((reference, "__self".into()));
    }
    let (target, r) = uses.rsplit_once('@').ok_or_else(|| format!("line {line}: `uses: {uses}` has no @REF"))?;
    if r.is_empty() || r.contains(char::is_whitespace) {
        return Err(format!("line {line}: `uses: {uses}` has no valid @REF"));
    }
    let mut parts = target.splitn(3, '/');
    let (Some(owner), Some(repo)) = (parts.next(), parts.next()) else {
        return Err(format!("line {line}: `uses: {uses}` is not OWNER/REPO@REF"));
    };
    if legacy_artifacts(owner, repo, r) {
        return Err(format!(
            "line {line}: `uses: {uses}`: GitHub refuses v3 and older of the artifact actions since 2025-01-30; use v4"
        ));
    }
    let mut reference =
        json!({ "type": "repository", "repositoryType": "GitHub", "name": format!("{owner}/{repo}"), "ref": r });
    if let Some(path) = parts.next().filter(|p| !p.is_empty()) {
        reference["path"] = json!(path);
    }
    Ok((reference, format!("__{owner}_{repo}")))
}

/// An artifact action of the legacy routes, which GitHub turned off (the
/// GitHub changelog, 2024-04-16): the local mode serves only v4 (T-119).
fn legacy_artifacts(owner: &str, repo: &str, r: &str) -> bool {
    let major = r.strip_prefix('v').and_then(|v| v.split('.').next()).and_then(|m| m.parse::<u32>().ok());
    owner == "actions" && matches!(repo, "upload-artifact" | "download-artifact") && major.is_some_and(|m| m <= 3)
}

fn step(
    index: usize,
    s: &WorkflowStep,
    d: &RunDefaults,
    unnamed: &mut BTreeMap<String, usize>,
) -> Result<Value, String> {
    let line = s.mark.line;
    let (reference, base, display, inputs) = match (&s.uses, &s.run) {
        (Some(uses), None) if uses.starts_with("actions/checkout@") => {
            let path = match s.with.as_ref().and_then(|w| match &w.kind {
                Kind::Map(e) => e.iter().find(|(k, _)| k.name == "path").map(|(_, v)| v),
                _ => None,
            }) {
                None => String::new(),
                Some(Node { kind: Kind::String(p), .. }) if !p.contains("${{") && !p.split('/').any(|c| c == "..") => {
                    p.clone()
                }
                Some(_) => {
                    return Err(format!("line {line}: the `path` of the local checkout must be a plain relative path"));
                }
            };
            (
                json!({ "type": "script" }),
                "__actions_checkout".to_string(),
                format!("Run {uses} (the local tree)"),
                vec![("script".to_string(), token(&checkout_script(&path))?)],
            )
        }
        (Some(uses), None) => {
            let (reference, base) = action(uses, line)?;
            let with = match s.with.as_ref().map(|n| &n.kind) {
                None => Vec::new(),
                Some(Kind::Map(entries)) => {
                    entries.iter().map(|(k, v)| Ok((k.name.clone(), scalar(v)?))).collect::<Result<_, String>>()?
                }
                Some(_) => return Err(format!("line {line}: `with` must be a mapping")),
            };
            (reference, base, format!("Run {uses}"), with)
        }
        (None, Some(script)) => {
            let mut inputs = vec![("script".to_string(), token(script)?)];
            if let Some(shell) = s.shell.as_deref().or(d.shell) {
                inputs.push(("shell".into(), token(shell)?));
            }
            if let Some(dir) = s.working_directory.as_deref().or(d.working_directory) {
                inputs.push(("workingDirectory".into(), token(dir)?));
            }
            let display = format!("Run {}", script.lines().next().unwrap_or("").trim());
            (json!({ "type": "script" }), "__run".to_string(), display, inputs)
        }
        _ => return Err(format!("line {line}: a step needs one of `run` and `uses`")),
    };
    // GitHub names a step with no id from its kind: `__run`, then
    // `__run_2`; `__actions_checkout`, then `__actions_checkout_2`.
    let context = match &s.id {
        Some(id) => id.clone(),
        None => {
            let n = unnamed.entry(base.clone()).or_default();
            *n += 1;
            if *n == 1 { base } else { format!("{base}_{n}") }
        }
    };
    let display = s.name.clone().unwrap_or(display);
    let mut v = json!({
        "type": "action", "reference": reference,
        "id": guid(0x100 + index as u64), "name": context, "contextName": context,
        "displayName": display, "condition": step_condition(s.condition.as_deref().unwrap_or("")),
        "inputs": mapping(inputs),
    });
    let environment = env(s.env.as_ref())?;
    if !environment.is_empty() {
        v["environment"] = mapping(environment);
    }
    if let Some(c) = &s.continue_on_error {
        v["continueOnError"] = scalar(c)?;
    }
    if let Some(t) = &s.timeout_minutes {
        v["timeoutInMinutes"] = scalar(t)?;
    }
    Ok(v)
}

/// The job of the fake broker for `cell` of `workflow`. `github`, `vars`
/// and `inputs` are the contexts of the run; the entries of `github` join
/// the canned ones of the broker, so `api_url` of the broker stays.
pub fn broker_job(
    workflow: &pod_actions_workflow::Workflow,
    cell: &Cell,
    github: &XValue,
    vars: &XValue,
    inputs: &XValue,
) -> Result<Job, String> {
    let job: &WorkflowJob = workflow.job(&cell.job).ok_or_else(|| format!("no job {}", cell.job))?;
    if let Some(uses) = &job.uses {
        return Err(format!("job {}: a reusable workflow (`uses: {uses}`) is not supported", job.id));
    }
    let mut containers = Map::new();
    if let Some(c) = &job.container {
        container_image(c, "container")?;
        containers.insert("jobContainer".into(), node_token(c)?);
    }
    if let Some(s) = &job.services {
        let Kind::Map(entries) = &s.kind else {
            return Err(format!("line {}: services must be a mapping", s.mark.line));
        };
        for (name, service) in entries {
            container_image(service, &format!("service {}", name.name))?;
        }
        containers.insert("jobServiceContainers".into(), node_token(s)?);
    }
    let wd = workflow.defaults.as_ref();
    let jd = job.defaults.as_ref();
    let defaults = RunDefaults {
        shell: jd.and_then(|d| d.shell.as_deref()).or(wd.and_then(|d| d.shell.as_deref())),
        working_directory: jd
            .and_then(|d| d.working_directory.as_deref())
            .or(wd.and_then(|d| d.working_directory.as_deref())),
    };
    let mut unnamed = BTreeMap::new();
    let steps: Vec<Value> =
        job.steps.iter().enumerate().map(|(i, s)| step(i, s, &defaults, &mut unnamed)).collect::<Result<_, _>>()?;
    // The runner merges the mappings in order: the job's env over the
    // workflow's.
    let mut environment = Vec::new();
    for e in [env(workflow.env.as_ref())?, env(job.env.as_ref())?] {
        if !e.is_empty() {
            environment.push(mapping(e));
        }
    }
    let outputs: Vec<(String, Value)> =
        job.outputs.iter().map(|(k, t)| Ok((k.clone(), token(t)?))).collect::<Result<_, String>>()?;
    let mut context = Map::new();
    let i = &cell.instance;
    for (name, value) in [("matrix", &i.matrix), ("strategy", &i.strategy), ("needs", &i.needs), ("vars", vars)] {
        context.insert(name.into(), context_data(value));
    }
    context.insert("inputs".into(), context_data(inputs));
    context.insert("github".into(), context_data(github));
    let mut b = Job::scripts(&i.name, &[]);
    b.overrides.insert("steps".into(), Value::Array(steps));
    b.overrides.insert("environmentVariables".into(), Value::Array(environment));
    if !outputs.is_empty() {
        b.overrides.insert("jobOutputs".into(), mapping(outputs));
    }
    b.overrides.insert("contextData".into(), Value::Object(context));
    b.overrides.extend(containers);
    Ok(b)
}

#[cfg(test)]
mod tests;
