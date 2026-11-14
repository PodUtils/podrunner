//! The `github` context of a local run (T-125): the facts of the git tree
//! of `--checkout` (or of the current directory), the flags that replace
//! them, and the payload of the event (`--event-path`, or a minimal payload
//! as GitHub sends it).

use std::path::Path;
use std::process::Command;

use pod_actions_expr::Value;
use pod_actions_workflow::{Kind, Node};
use serde_json::json;

use super::Args;

/// The facts of the run that the tree or the flags give.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Facts {
    pub repository: String,
    pub git_ref: String,
    pub sha: String,
    pub actor: String,
    pub default_branch: String,
    /// `github.event.inputs` of a `workflow_dispatch` (T-126).
    pub inputs: serde_json::Map<String, serde_json::Value>,
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !text.is_empty()).then_some(text)
}

/// `OWNER/REPO` of a remote URL: `https://HOST/OWNER/REPO(.git)`,
/// `git@HOST:OWNER/REPO(.git)` or `ssh://git@HOST/OWNER/REPO`.
pub(super) fn repository_of(url: &str) -> Option<String> {
    let path = url.trim_end_matches('/').trim_end_matches(".git");
    let path = match path.split_once("://") {
        Some((_, rest)) => rest.split_once('/')?.1,
        None => path.split_once(':')?.1,
    };
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let [.., owner, repo] = parts[..] else { return None };
    Some(format!("{owner}/{repo}"))
}

/// The facts of the tree, then the flags over them.
pub(super) fn facts(a: &Args) -> Facts {
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let dir = a.checkout.as_deref().unwrap_or(&cwd);
    let remote = a.remote.as_deref().unwrap_or("origin");
    let default_branch = git(dir, &["symbolic-ref", "-q", &format!("refs/remotes/{remote}/HEAD")])
        .and_then(|r| r.rsplit('/').next().map(str::to_string))
        .unwrap_or_else(|| "main".into());
    let tree_ref = git(dir, &["symbolic-ref", "-q", "HEAD"])
        .or_else(|| git(dir, &["describe", "--exact-match", "--tags", "HEAD"]).map(|t| format!("refs/tags/{t}")));
    Facts {
        repository: a
            .repository
            .clone()
            .or_else(|| git(dir, &["remote", "get-url", remote]).and_then(|u| repository_of(&u)))
            .unwrap_or_else(|| "local/workflow".into()),
        git_ref: a.git_ref.clone().or(tree_ref).unwrap_or_else(|| format!("refs/heads/{default_branch}")),
        sha: a.sha.clone().or_else(|| git(dir, &["rev-parse", "HEAD"])).unwrap_or_else(|| "0".repeat(40)),
        actor: a.actor.clone().or_else(|| git(dir, &["config", "user.name"])).unwrap_or_else(|| "local".into()),
        default_branch,
        inputs: serde_json::Map::new(),
    }
}

/// The short name and the type of a ref.
fn ref_parts(r: &str) -> (&str, &str) {
    if let Some(t) = r.strip_prefix("refs/tags/") {
        (t, "tag")
    } else {
        (r.strip_prefix("refs/heads/").unwrap_or(r), "branch")
    }
}

fn get<'a>(n: &'a Node, key: &str) -> Option<&'a Node> {
    match &n.kind {
        Kind::Map(e) => e.iter().find(|(k, _)| k.name == key).map(|(_, v)| v),
        _ => None,
    }
}

fn scalar(n: &Node) -> Option<String> {
    match &n.kind {
        Kind::String(s) => Some(s.clone()),
        Kind::Bool(b) => Some(b.to_string()),
        Kind::Number(x) if x.fract() == 0.0 => Some(format!("{x:.0}")),
        Kind::Number(x) => Some(x.to_string()),
        _ => None,
    }
}

/// The typed inputs, and the inputs of `github.event.inputs`.
pub(super) type Inputs = (Vec<(String, Value)>, serde_json::Map<String, serde_json::Value>);

/// The inputs of a `workflow_dispatch` (T-126): each value of `--input`
/// over the default of its input, with the type of GitHub (a boolean is a
/// boolean, a number a number); a `required` input with no value, a value
/// outside a `choice`, and an input that `on:` does not declare fail the
/// run. Also the inputs as `github.event.inputs` gives them: strings.
pub(super) fn dispatch_inputs(on: &Node, given: &[(String, String)]) -> Result<Inputs, String> {
    let declared = get(on, "workflow_dispatch").and_then(|w| get(w, "inputs"));
    let Some(Node { kind: Kind::Map(inputs), .. }) = declared else {
        let typed = given.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
        let event = given.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
        return Ok((typed, event));
    };
    for (k, _) in given {
        if !inputs.iter().any(|(name, _)| &name.name == k) {
            return Err(format!("--input {k}: on.workflow_dispatch.inputs has no such input"));
        }
    }
    let (mut typed, mut event) = (Vec::new(), serde_json::Map::new());
    for (name, spec) in inputs {
        let name = name.name.clone();
        let value = given
            .iter()
            .rev()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.clone())
            .or_else(|| get(spec, "default").and_then(scalar));
        let required = matches!(get(spec, "required").map(|n| &n.kind), Some(Kind::Bool(true)));
        let kind = get(spec, "type").and_then(scalar).unwrap_or_else(|| "string".into());
        let Some(value) = value else {
            if required {
                return Err(format!("the input {name} is required: give it with --input {name}=VALUE"));
            }
            // GitHub gives an input with no value and no default as empty.
            let empty = if kind == "boolean" { Value::Bool(false) } else { Value::String(String::new()) };
            typed.push((name.clone(), empty));
            event.insert(name, json!(""));
            continue;
        };
        let v = match kind.as_str() {
            "boolean" => match value.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => return Err(format!("the input {name} is a boolean, not {value:?}")),
            },
            "number" => {
                Value::Number(value.parse().map_err(|_| format!("the input {name} is a number, not {value:?}"))?)
            }
            "choice" => {
                let options: Vec<String> = match get(spec, "options").map(|n| &n.kind) {
                    Some(Kind::Seq(o)) => o.iter().filter_map(scalar).collect(),
                    _ => Vec::new(),
                };
                if !options.contains(&value) {
                    return Err(format!("the input {name} takes one of {options:?}, not {value:?}"));
                }
                Value::String(value.clone())
            }
            _ => Value::String(value.clone()),
        };
        typed.push((name.clone(), v));
        event.insert(name, json!(value));
    }
    Ok((typed, event))
}

/// The minimal payload of `event`, as GitHub sends it.
pub(super) fn payload(event: &str, f: &Facts) -> serde_json::Value {
    let (owner, name) = f.repository.split_once('/').unwrap_or(("local", &f.repository));
    let repository = json!({
        "full_name": f.repository, "name": name, "owner": { "login": owner },
        "default_branch": f.default_branch,
    });
    let (ref_name, _) = ref_parts(&f.git_ref);
    match event {
        "push" => json!({
            "ref": f.git_ref, "before": "0".repeat(40), "after": f.sha,
            "head_commit": { "id": f.sha }, "pusher": { "name": f.actor }, "repository": repository,
        }),
        "pull_request" | "pull_request_target" => json!({
            "action": "opened", "number": 1,
            "pull_request": {
                "number": 1,
                "head": { "ref": ref_name, "sha": f.sha },
                "base": { "ref": f.default_branch, "sha": f.sha },
            },
            "repository": repository,
        }),
        "workflow_dispatch" => json!({ "ref": f.git_ref, "inputs": f.inputs, "repository": repository }),
        _ => json!({ "repository": repository }),
    }
}

/// The `github` context of the job level.
pub(super) fn github(a: &Args, f: &Facts) -> Result<Value, String> {
    let event = match &a.event_path {
        Some(p) => std::fs::read_to_string(p).map_err(|e| format!("--event-path {}: {e}", p.display()))?,
        None => payload(&a.event, f).to_string(),
    };
    let event = Value::from_json(&event).map_err(|e| format!("the event payload: {e}"))?;
    let workflow = a.file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let owner = f.repository.split('/').next().unwrap_or("local").to_string();
    let pull = matches!(a.event.as_str(), "pull_request" | "pull_request_target");
    let (ref_name, ref_type) = ref_parts(&f.git_ref);
    let s = |v: &str| Value::String(v.into());
    let mut entries = vec![
        ("event_name", s(&a.event)),
        ("event", event),
        ("workflow", s(&workflow)),
        ("repository", s(&f.repository)),
        ("repository_owner", s(&owner)),
        ("server_url", s("https://github.com")),
        ("run_id", s("1")),
        ("run_number", s("1")),
        ("run_attempt", s("1")),
        ("sha", s(&f.sha)),
        ("actor", s(&f.actor)),
        ("triggering_actor", s(&f.actor)),
    ];
    if pull {
        // A pull request runs on its merge ref; head and base name the branches.
        entries.extend([
            ("ref", s("refs/pull/1/merge")),
            ("ref_name", s("1/merge")),
            ("ref_type", s("branch")),
            ("head_ref", s(ref_name)),
            ("base_ref", s(&f.default_branch)),
        ]);
    } else {
        entries.extend([("ref", s(&f.git_ref)), ("ref_name", s(ref_name)), ("ref_type", s(ref_type))]);
    }
    Ok(Value::object(entries))
}

/// The events that `on:` takes: a name, a list of names, or the keys of a
/// mapping.
pub(super) fn events(on: &Node) -> Vec<String> {
    match &on.kind {
        Kind::String(e) => vec![e.clone()],
        Kind::Seq(items) => items
            .iter()
            .filter_map(|n| match &n.kind {
                Kind::String(e) => Some(e.clone()),
                _ => None,
            })
            .collect(),
        Kind::Map(entries) => entries.iter().map(|(k, _)| k.name.clone()).collect(),
        _ => Vec::new(),
    }
}

/// The changed files of the tree since `since`: `git diff --name-only`
/// against the working tree (T-125).
fn changed(a: &Args, since: &str) -> Result<Vec<String>, String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let dir = a.checkout.as_deref().unwrap_or(&cwd);
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["diff", "--name-only", since, "--"])
        .output()
        .map_err(|e| format!("--changed-since: git: {e}"))?;
    if !out.status.success() {
        return Err(format!("--changed-since {since}: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect())
}

/// The workflow takes the event of the run after the filters of its `on:`
/// (`types`, `branches`, `tags`, `paths` and their `-ignore` forms), as
/// GitHub decides (T-125): the activity type of the payload, the ref of the
/// tree (the base branch for a pull request), and the changed files of
/// `--changed-since`; with no `--changed-since`, a `paths` filter does not
/// apply.
pub(super) fn takes(a: &Args, f: &Facts, w: &pod_actions_workflow::Workflow) -> Result<bool, String> {
    let payload: Option<serde_json::Value> = match &a.event_path {
        Some(p) => std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(&t).ok()),
        None => Some(payload(&a.event, f)),
    };
    let action = payload.as_ref().and_then(|p| p["action"].as_str().map(str::to_string));
    let pull = matches!(a.event.as_str(), "pull_request" | "pull_request_target");
    let git_ref = if pull { format!("refs/heads/{}", f.default_branch) } else { f.git_ref.clone() };
    let paths = a.changed_since.as_deref().map(|s| changed(a, s)).transpose()?;
    let workflow = payload.as_ref().and_then(|p| p["workflow"]["name"].as_str().map(str::to_string));
    let trigger = pod_actions_workflow::Trigger {
        event: &a.event,
        action: action.as_deref(),
        git_ref: Some(&git_ref),
        paths: paths.as_deref(),
        workflow: workflow.as_deref(),
    };
    w.takes(&trigger).map_err(|e| e.to_string())
}
