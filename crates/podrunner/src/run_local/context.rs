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
        "workflow_dispatch" => json!({ "ref": f.git_ref, "inputs": {}, "repository": repository }),
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
