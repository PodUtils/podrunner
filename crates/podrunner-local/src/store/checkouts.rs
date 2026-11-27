//! The checkout steps of the composite actions of the store (T-718): the
//! run pins the trees of their static `repository` and `ref` before the
//! first job, as for a local composite action (T-713).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use pod_actions_workflow::{Kind, Limits, Node};

/// The deepest chain of composite actions that the run follows, as GitHub
/// follows composite actions to 10 levels.
const DEPTH: usize = 10;

/// A checkout step of an action of the store: the `uses:` of the action,
/// the line of the step in its `action.yml`, and its `with:`.
pub type Checkout = (String, u32, Option<Node>);

/// Each `uses: actions/checkout@REF` step of each composite action of the
/// store that `uses` names, and of each action of the store that one names.
/// `commits` gives the commit of each `OWNER/REPO@REF` in the store `dir`.
/// An action that the store does not have, or that is not composite, has
/// none: its job fails at its start, as on GitHub.
pub fn action_checkouts(uses: &[String], dir: &Path, commits: &BTreeMap<String, String>) -> Vec<Checkout> {
    let mut queue: VecDeque<(String, usize)> = uses.iter().map(|u| (u.clone(), 1)).collect();
    let (mut seen, mut out) = (BTreeSet::new(), Vec::new());
    while let Some((u, depth)) = queue.pop_front() {
        if depth > DEPTH || !seen.insert(u.clone()) {
            continue;
        }
        for s in steps(&u, dir, commits) {
            let Some(Kind::String(inner)) = s.get("uses").map(|n| &n.kind) else { continue };
            if inner.starts_with("actions/checkout@") {
                out.push((u.clone(), s.mark.line, s.get("with").cloned()));
            } else if !inner.starts_with("./") && !inner.starts_with("docker://") {
                queue.push_back((inner.clone(), depth + 1));
            }
        }
    }
    out
}

/// The steps of the composite action `OWNER/REPO[/PATH]@REF` of the store;
/// none for another kind, or for an action that the store does not have.
fn steps(uses: &str, dir: &Path, commits: &BTreeMap<String, String>) -> Vec<Node> {
    let Some((left, r)) = uses.rsplit_once('@') else { return Vec::new() };
    let mut parts = left.splitn(3, '/');
    let (Some(owner), Some(repo)) = (parts.next(), parts.next()) else { return Vec::new() };
    let repo = format!("{owner}/{repo}");
    let full = crate::lock::is_commit(r).then_some(r);
    let Some(commit) = commits.get(&format!("{repo}@{r}")).map(String::as_str).or(full) else { return Vec::new() };
    let path = parts.next().unwrap_or("");
    let Some(text) = crate::exec::docker::action_text(dir, &repo, commit, path) else { return Vec::new() };
    let Ok(root) = pod_actions_workflow::yaml::load(&text, Limits::default()) else { return Vec::new() };
    let Some(runs) = root.get("runs") else { return Vec::new() };
    if !matches!(runs.get("using").map(|n| &n.kind), Some(Kind::String(u)) if u == "composite") {
        return Vec::new();
    }
    match runs.get("steps").map(|n| &n.kind) {
        Some(Kind::Seq(steps)) => steps.clone(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
