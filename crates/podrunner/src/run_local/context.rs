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
#[path = "ids.rs"]
mod ids;

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

/// A `--remote NAME` that the tree does not have (T-277).
pub(super) fn missing_remote(a: &Args) -> Option<String> {
    let name = a.remote.as_deref()?;
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let dir = a.checkout.as_deref().unwrap_or(&cwd);
    git(dir, &["remote", "get-url", name]).is_none().then(|| format!("--remote {name}: the tree has no such remote"))
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
    // GitHub runs a schedule on the newest commit of the default branch
    // (T-433); the files stay those of the tree.
    let schedule = a.event == "schedule";
    let tree_ref = if schedule { None } else { tree_ref };
    let head = match schedule {
        true => git(dir, &["rev-parse", "-q", "--verify", &format!("refs/heads/{default_branch}")])
            .or_else(|| git(dir, &["rev-parse", "HEAD"])),
        false => git(dir, &["rev-parse", "HEAD"]),
    };
    Facts {
        repository: a
            .repository
            .clone()
            .or_else(|| git(dir, &["remote", "get-url", remote]).and_then(|u| repository_of(&u)))
            .unwrap_or_else(|| "local/workflow".into()),
        git_ref: a.git_ref.clone().or(tree_ref).unwrap_or_else(|| format!("refs/heads/{default_branch}")),
        sha: a.sha.clone().or(head).unwrap_or_else(|| "0".repeat(40)),
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
        // A workflow_dispatch with no inputs declares none (#231).
        let names = match &on.kind {
            Kind::String(e) => e == "workflow_dispatch",
            Kind::Seq(items) => items.iter().any(|i| matches!(&i.kind, Kind::String(e) if e == "workflow_dispatch")),
            Kind::Map(m) => m.iter().any(|(k, _)| k.name == "workflow_dispatch"),
            _ => false,
        };
        if let Some((k, _)) = given.first().filter(|_| names) {
            return Err(format!("--input {k}: on.workflow_dispatch declares no inputs"));
        }
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

/// The cron of the next run of `--schedule`: `github.event.schedule`.
static SCHEDULE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

pub(super) fn set_schedule(cron: &str) {
    *SCHEDULE.lock().unwrap_or_else(|e| e.into_inner()) = cron.to_string();
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
                // The head is a branch of the same repository, not a fork,
                // so a guard of a fork (`head.repo.full_name ==
                // github.repository`) runs as on GitHub (T-424).
                "head": { "ref": ref_name, "sha": f.sha, "repo": repository.clone() },
                "base": { "ref": f.default_branch, "sha": f.sha, "repo": repository.clone() },
                "user": { "login": f.actor }, "draft": false,
            },
            "repository": repository,
        }),
        "workflow_dispatch" => json!({ "ref": f.git_ref, "inputs": f.inputs, "repository": repository }),
        "schedule" => json!({
            "schedule": SCHEDULE.lock().unwrap_or_else(|e| e.into_inner()).clone(), "repository": repository,
        }),
        _ => json!({ "repository": repository }),
    }
}

/// The `github.workflow` of the file, and its path in the repository, as
/// GitHub gives them (T-426): its `name:`, else the path; the path
/// `.github/workflows/FILE` for a file of that directory, else the name of
/// the file.
fn workflow_of(file: &std::path::Path) -> (String, String) {
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let named = |d: Option<&std::path::Path>, n: &str| d.and_then(|d| d.file_name()).is_some_and(|x| x == n);
    let dir = file.parent();
    let path = match named(dir, "workflows") && named(dir.and_then(|d| d.parent()), ".github") {
        true => format!(".github/workflows/{name}"),
        false => name,
    };
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let title = pod_actions_workflow::parse(&text).ok().and_then(|p| p.workflow.name).filter(|n| !n.is_empty());
    (title.unwrap_or_else(|| path.clone()), path)
}

/// The `github` context of the job level.
pub(super) fn github(a: &Args, f: &Facts) -> Result<Value, String> {
    let event = match &a.event_path {
        Some(p) => std::fs::read_to_string(p).map_err(|e| format!("--event-path {}: {e}", p.display()))?,
        None => payload(&a.event, f).to_string(),
    };
    let at = a
        .event_path
        .as_ref()
        .map_or_else(|| "the event payload".to_string(), |p| format!("--event-path {}", p.display()));
    let base = base_branch(serde_json::from_str(&event).ok().as_ref(), f);
    let event = Value::from_json(&event).map_err(|e| format!("{at}: {e}"))?;
    let (workflow, path) = workflow_of(&a.file);
    let owner = f.repository.split('/').next().unwrap_or("local").to_string();
    let pull = matches!(a.event.as_str(), "pull_request" | "pull_request_target");
    let (ref_name, ref_type) = ref_parts(&f.git_ref);
    // A new id and number for each run, and an attempt for each rerun
    // (T-418).
    let (run_id, run_number, run_attempt) = ids::ids(a, ids::counter(&f.repository, &a.file).as_deref());
    let s = |v: &str| Value::String(v.into());
    let mut entries = vec![
        ("event_name", s(&a.event)),
        ("event", event),
        ("workflow", s(&workflow)),
        ("workflow_ref", s(&format!("{}/{path}@{}", f.repository, f.git_ref))),
        ("workflow_sha", s(&f.sha)),
        ("repository", s(&f.repository)),
        ("repository_owner", s(&owner)),
        ("server_url", s("https://github.com")),
        ("run_id", s(&run_id)),
        ("run_number", s(&run_number)),
        ("run_attempt", s(&run_attempt)),
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
            ("base_ref", s(&base)),
        ]);
    } else {
        entries.extend([("ref", s(&f.git_ref)), ("ref_name", s(ref_name)), ("ref_type", s(ref_type))]);
    }
    Ok(Value::object(entries))
}

/// The events that `on:` takes: a name, a list of names, or the keys of a
/// mapping.
/// The refusal of a run whose `on:` does not take its event: an event that
/// `on:` does not name needs `--event`; one that a filter refuses, another
/// ref or file list (T-339).
pub(super) fn refused_event(a: &super::Args, git_ref: &str, events: &[String]) -> String {
    let fix = match events.contains(&a.event) {
        true => {
            "a filter refuses it: --ref REF or --changed-since REF gives others, and --force-event runs it".to_string()
        }
        false => format!("give --event with one of {events:?}, or --force-event"),
    };
    format!("{}: `on:` ({events:?}) does not take the event {:?} of {git_ref}; {fix}", a.file.display(), a.event)
}

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
/// against the working tree (T-125), and each new file that git does not
/// ignore, as the checkout has it (T-430). Each path is from the top of
/// the tree, as a `paths` filter reads it.
fn changed(a: &Args, since: &str) -> Result<Vec<String>, String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let dir = a.checkout.as_deref().unwrap_or(&cwd);
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .map_err(|e| format!("--changed-since: git: {e}"))?;
        if !out.status.success() {
            return Err(format!("--changed-since {since}: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect::<Vec<_>>())
    };
    super::args::git_value("--changed-since", since.to_string())?;
    // `--end-of-options` (git 2.24) is the second guard; an older git
    // refuses it, and the check above holds there.
    let mut files = git(&["diff", "--name-only", "--end-of-options", since, "--"])
        .or_else(|e| if e.contains("end-of-options") { git(&["diff", "--name-only", since, "--"]) } else { Err(e) })?;
    files.extend(git(&["ls-files", "--others", "--exclude-standard", "--full-name"])?);
    files.sort();
    files.dedup();
    Ok(files)
}

#[cfg(test)]
mod changed_tests {
    /// T-430: a new file that git does not ignore is a changed file; an
    /// ignored one is not.
    #[test]
    fn a_new_file_is_a_changed_file() {
        let dir = std::env::temp_dir().join(format!("podrunner-changed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git").arg("-C").arg(&dir).args(args).status().unwrap().success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::fs::write(dir.join(".gitignore"), "*.log\n").unwrap();
        git(&["add", "-A"]);
        git(&["-c", "user.name=t", "-c", "user.email=t@example.invalid", "commit", "-qm", "c"]);
        std::fs::write(dir.join("a.txt"), "b").unwrap();
        std::fs::write(dir.join("sub/new.rs"), "n").unwrap();
        std::fs::write(dir.join("x.log"), "l").unwrap();
        let a = super::Args { checkout: Some(dir.join("sub")), ..super::Args::default() };
        assert_eq!(super::changed(&a, "HEAD").unwrap(), ["a.txt", "sub/new.rs"]);
        // T-631: a value that starts with `-` is never an option of git,
        // also when no parse of the flags checked it.
        let file = dir.join("INJECT");
        let e = super::changed(&a, &format!("--output={}", file.display())).unwrap_err();
        assert!(e.contains("starts with '-'"), "{e}");
        assert!(!file.exists(), "git wrote the file of --output");
        let _ = std::fs::remove_dir_all(&dir);
    }
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
    // A pull request is filtered on its base branch, as GitHub does (#230).
    let git_ref = if pull { format!("refs/heads/{}", base_branch(payload.as_ref(), f)) } else { f.git_ref.clone() };
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

/// The base branch of a pull request: `pull_request.base.ref` of the
/// payload, else the default branch of the tree (#230).
fn base_branch(payload: Option<&serde_json::Value>, f: &Facts) -> String {
    payload
        .and_then(|p| p["pull_request"]["base"]["ref"].as_str())
        .map(|r| r.trim_start_matches("refs/heads/").to_string())
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| f.default_branch.clone())
}

#[cfg(test)]
mod workflow_tests {
    /// T-426: `github.workflow` is the `name:`, else the path in the
    /// repository.
    #[test]
    fn the_name_and_the_path_of_a_workflow() {
        let dir = std::env::temp_dir().join(format!("podrunner-workflow-of-{}", std::process::id()));
        let wf = dir.join(".github/workflows");
        std::fs::create_dir_all(&wf).unwrap();
        let job = "jobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
        std::fs::write(wf.join("ci.yml"), format!("name: CI build\non: push\n{job}")).unwrap();
        std::fs::write(wf.join("plain.yml"), format!("on: push\n{job}")).unwrap();
        std::fs::write(dir.join("loose.yml"), format!("on: push\n{job}")).unwrap();
        let of = |p: std::path::PathBuf| super::workflow_of(&p);
        assert_eq!(of(wf.join("ci.yml")), ("CI build".into(), ".github/workflows/ci.yml".into()));
        assert_eq!(
            of(wf.join("plain.yml")),
            (".github/workflows/plain.yml".into(), ".github/workflows/plain.yml".into())
        );
        assert_eq!(of(dir.join("loose.yml")), ("loose.yml".into(), "loose.yml".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
