//! The stand-in of `actions/checkout` in the local mode (T-092, T-133,
//! T-425). It is the action `podrunner/checkout` of the fake broker
//! (T-697): the log of the job shows its inputs, not a script.

use pod_actions_workflow::{Kind, Node, Step as WorkflowStep};

use std::collections::BTreeMap;

use crate::tree::refs::ask;

/// The inputs of the action for the step `s`, and what it checks out.
pub(super) fn stand_in(
    s: &WorkflowStep,
    pins: &BTreeMap<String, String>,
    self_repo: &str,
) -> Result<(Inputs, String), String> {
    let (inputs, what) = stand_in_of(s, pins, self_repo)?;
    // The keys that the stand-in does not take stay in the display (T-643).
    let keys: Vec<&str> = crate::tree::ignored::ignored(s).into_iter().map(|(k, _)| k).collect();
    let what = if keys.is_empty() { what } else { format!("{what}; ignores {}", keys.join(", ")) };
    Ok((inputs, what))
}

fn stand_in_of(
    s: &WorkflowStep,
    pins: &BTreeMap<String, String>,
    self_repo: &str,
) -> Result<(Inputs, String), String> {
    let line = s.mark.line;
    let with = |name: &str| {
        s.with.as_ref().and_then(|w| match &w.kind {
            Kind::Map(e) => e.iter().find(|(k, _)| k.name == name).map(|(_, v)| v),
            _ => None,
        })
    };
    let path = match with("path") {
        None => String::new(),
        Some(Node { kind: Kind::String(p), .. }) if !p.contains("${{") && !p.split('/').any(|c| c == "..") => p.clone(),
        Some(_) => return Err(format!("line {line}: the `path` of the local checkout must be a plain relative path")),
    };
    let level = match with("submodules").map(|n| &n.kind) {
        None | Some(Kind::Bool(false)) => 0,
        Some(Kind::Bool(true)) => 1,
        Some(Kind::String(v)) if v == "false" => 0,
        Some(Kind::String(v)) if v == "true" => 1,
        Some(Kind::String(v)) if v == "recursive" => crate::tree::SUBMODULE_LEVELS,
        Some(_) => {
            return Err(format!(
                "line {line}: the `submodules` of the local checkout must be true, false or recursive"
            ));
        }
    };
    let target = crate::store::checkout_target(s.with.as_ref(), self_repo).map_err(|e| format!("line {line}: {e}"))?;
    match &target {
        None => same_repo(with("ref"), &path, level, pins, line),
        Some(t) if level > 0 => Err(format!(
            "line {line}: the checkout of `{t}` with `submodules`: the store keeps no submodule of another repository"
        )),
        Some(t) => {
            let sha = pins.get(t).ok_or_else(|| format!("line {line}: the checkout of `{t}` is not pinned"))?;
            let repo = t.split('@').next().unwrap_or(t);
            Ok((inputs(&path, Route::Other(repo, sha), 0), format!("{t} at {sha}")))
        }
    }
}

/// A checkout of the same repository: the local tree, or with `ref:` the
/// commit that the run resolved in the local git (T-640). A ref with no
/// commit fails the step with its reason, as GitHub fails it.
fn same_repo(
    node: Option<&Node>,
    path: &str,
    level: u32,
    pins: &BTreeMap<String, String>,
    line: u32,
) -> Result<(Inputs, String), String> {
    use crate::tree::refs::{Ask, key};
    let tree = |what: String| Ok((inputs(path, Route::Tree, level), what));
    let r = match ask(node) {
        Ask::Default => return tree("the local tree".into()),
        Ask::Expression(e) => return tree(format!("the local tree; `ref: {e}` is an expression")),
        Ask::Bad(why) => return Ok((fail(&why), why)),
        Ask::Ref(r) => r,
    };
    match pins.get(&key(&r)).map(String::as_str) {
        Some("") => tree(format!("the local tree, the commit of `ref: {r}`")),
        Some(why) if why.starts_with('!') => Ok((fail(&why[1..]), why[1..].to_string())),
        Some(_) if level > 0 => Err(format!(
            "line {line}: the checkout of `ref: {r}` with `submodules`: the stand-in gives submodules only in the local tree"
        )),
        Some(sha) => Ok((inputs(path, Route::Commit(sha), 0), format!("ref {r} at {sha}"))),
        None => {
            let why = format!(
                "`ref: {r}`: the run resolved no commit for it; fix: give --checkout DIR, a git tree that has it"
            );
            Ok((fail(&why), why))
        }
    }
}

/// The inputs of the action `podrunner/checkout`, in order.
pub(super) type Inputs = Vec<(String, String)>;

/// The step fails with one line on stderr. An expression in the text
/// stays text (an expression of the ref stays the local tree).
fn fail(why: &str) -> Inputs {
    vec![("fail".into(), why.replace("${{", "$ {{"))]
}

/// Where the stand-in takes its tarball from.
enum Route<'a> {
    /// The local tree.
    Tree,
    /// A commit of the local git (T-640).
    Commit(&'a str),
    /// The tree of `OWNER/REPO` at its commit from the store (T-133).
    Other(&'a str, &'a str),
}

/// The inputs for a tarball of the fake broker (`github.podrunner_broker`
/// names it) into the workspace, or into `path` in it. With another
/// repository (`OWNER/REPO` and its commit, T-133), the tarball of that
/// commit from the store, with its top directory stripped. The local tree
/// keeps the submodules to the depth `level` (T-425).
fn inputs(path: &str, route: Route, level: u32) -> Inputs {
    use podrunner_broker_fake::actions::{CHECKOUT, archive_name, checkout_at};
    let (url, strip) = match route {
        Route::Tree => (CHECKOUT.to_string(), ""),
        Route::Commit(sha) => (checkout_at(sha), ""),
        Route::Other(repo, sha) => (format!("_actions/{}", archive_name(repo, sha)), "1"),
    };
    let mut v: Vec<(&str, String)> =
        vec![("broker", "${{ github.podrunner_broker }}".into()), ("route", url), ("dest", path.into())];
    if !strip.is_empty() {
        v.push(("strip", strip.into()));
    }
    if matches!(route, Route::Tree) {
        v.push(("list", crate::tree::SUBMODULES.into()));
        v.push(("level", level.to_string()));
    }
    v.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

#[cfg(test)]
mod tests;
