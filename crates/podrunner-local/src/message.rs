//! A cell of a workflow job into the job of the fake broker (T-088). GitHub
//! sends each `${{ }}` of a step, of `env` and of `outputs` as a token that
//! the runner evaluates in its own contexts (`steps`, `env`, `runner`,
//! `job`): a text that is one expression is a basic expression, and a text
//! with expressions around literal text is `format()`. The contexts of the
//! job level that the runner cannot know (`matrix`, `strategy`, `needs`,
//! `vars`, `inputs`) go into `contextData`.

use std::collections::BTreeMap;

use pod_actions_expr::{Template, Value as XValue};
use pod_actions_workflow::{Job as WorkflowJob, Kind, Node, Step as WorkflowStep};
use podrunner_broker_fake::Job;
use podrunner_broker_fake::message::step_condition;
use serde_json::{Map, Value, json};

use crate::Cell;

/// One part of a text: literal text, or the text of an expression.
#[derive(Debug, PartialEq)]
enum Part {
    Text(String),
    Expr(String),
}

/// The parts of `text`, with the scan of `Template::parse`: `}}` in a quoted
/// string does not end an expression.
fn split(text: &str) -> Vec<Part> {
    let chars: Vec<char> = text.chars().collect();
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut i = 0;
    while i < chars.len() {
        if !(chars[i] == '$' && chars.get(i + 1) == Some(&'{') && chars.get(i + 2) == Some(&'{')) {
            literal.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i + 3;
        let mut quoted = false;
        let mut j = start;
        let mut end = None;
        while j < chars.len() {
            if chars[j] == '\'' {
                quoted = !quoted;
            } else if !quoted && chars[j] == '}' && j > start && chars[j - 1] == '}' {
                end = Some(j - 1);
                break;
            }
            j += 1;
        }
        let Some(end) = end else {
            literal.extend(&chars[i..]);
            break;
        };
        if !literal.is_empty() {
            parts.push(Part::Text(std::mem::take(&mut literal)));
        }
        parts.push(Part::Expr(chars[start..end].iter().collect::<String>().trim().to_string()));
        i = end + 2;
    }
    if !literal.is_empty() {
        parts.push(Part::Text(literal));
    }
    parts
}

/// A text of the workflow as a template token.
pub fn token(text: &str) -> Result<Value, String> {
    let template = Template::parse(text).map_err(|e| format!("{text:?}: {e}"))?;
    if template.is_literal() {
        return Ok(json!(text));
    }
    if let Some(e) = template.as_expression() {
        return Ok(json!({ "type": 3, "expr": e.text().trim() }));
    }
    let parts = split(text);
    let exprs: Vec<&String> = parts
        .iter()
        .filter_map(|p| match p {
            Part::Expr(e) => Some(e),
            Part::Text(_) => None,
        })
        .collect();
    if exprs.len() != template.expressions().count() {
        return Err(format!("{text:?}: the parts do not agree with the template"));
    }
    let mut format = String::new();
    let mut n = 0;
    for p in &parts {
        match p {
            Part::Text(t) => format.push_str(&t.replace('\'', "''").replace('{', "{{").replace('}', "}}")),
            Part::Expr(_) => {
                format.push_str(&format!("{{{n}}}"));
                n += 1;
            }
        }
    }
    let args: String = exprs.iter().map(|e| format!(", ({e})")).collect();
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
/// The reference of `uses:`, its base context name and its display name.
/// An action of a repository must be pinned by a full commit id: the local
/// mode has no forge to resolve a tag.
fn action(uses: &str, line: u32) -> Result<(Value, String), String> {
    if uses.starts_with("docker://") {
        return Err(format!("line {line}: `uses: {uses}`: a Docker action is not supported yet"));
    }
    if let Some(path) = uses.strip_prefix("./") {
        let reference = json!({ "type": "repository", "repositoryType": "self", "path": format!("./{path}") });
        return Ok((reference, "__self".into()));
    }
    let (target, r) = uses.rsplit_once('@').ok_or_else(|| format!("line {line}: `uses: {uses}` has no @REF"))?;
    if r.len() != 40 || !r.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("line {line}: `uses: {uses}`: pin an action by a full commit id"));
    }
    let mut parts = target.splitn(3, '/');
    let (Some(owner), Some(repo)) = (parts.next(), parts.next()) else {
        return Err(format!("line {line}: `uses: {uses}` is not OWNER/REPO@SHA"));
    };
    let mut reference =
        json!({ "type": "repository", "repositoryType": "GitHub", "name": format!("{owner}/{repo}"), "ref": r });
    if let Some(path) = parts.next().filter(|p| !p.is_empty()) {
        reference["path"] = json!(path);
    }
    Ok((reference, format!("__{owner}_{repo}")))
}

fn step(
    index: usize,
    s: &WorkflowStep,
    d: &RunDefaults,
    unnamed: &mut BTreeMap<String, usize>,
) -> Result<Value, String> {
    let line = s.mark.line;
    let (reference, base, display, inputs) = match (&s.uses, &s.run) {
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

/// The job of the fake broker for `cell` of `workflow`. `vars` and `inputs`
/// are the contexts of the run.
pub fn broker_job(
    workflow: &pod_actions_workflow::Workflow,
    cell: &Cell,
    vars: &XValue,
    inputs: &XValue,
) -> Result<Job, String> {
    let job: &WorkflowJob = workflow.job(&cell.job).ok_or_else(|| format!("no job {}", cell.job))?;
    if let Some(uses) = &job.uses {
        return Err(format!("job {}: a reusable workflow (`uses: {uses}`) is not supported", job.id));
    }
    if job.container.is_some() || job.services.is_some() {
        return Err(format!("job {}: `container:` and `services:` are not supported yet (T-089)", job.id));
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
    let mut b = Job::scripts(&i.name, &[]);
    b.overrides.insert("steps".into(), Value::Array(steps));
    b.overrides.insert("environmentVariables".into(), Value::Array(environment));
    if !outputs.is_empty() {
        b.overrides.insert("jobOutputs".into(), mapping(outputs));
    }
    b.overrides.insert("contextData".into(), Value::Object(context));
    Ok(b)
}

#[cfg(test)]
mod tests;
