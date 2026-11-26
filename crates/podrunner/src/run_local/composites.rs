//! The `uses:` of each local composite action (`uses: ./PATH`) of a
//! workflow, and of each local action that one names (T-346): the lock pins
//! them before the first job, as it pins a direct `uses:`.

use std::collections::BTreeSet;
use std::path::Path;

use pod_actions_workflow::{Kind, Limits, Node};

/// The deepest chain of local actions that the run follows, as GitHub
/// follows composite actions to 10 levels.
const DEPTH: usize = 10;

/// Each remote `uses:` of the local actions of the steps of `w`, read from
/// the tree `base` (the checkout). An `action.yml` that is not there is
/// left out: its job fails at its start, as on GitHub.
pub(super) fn local_uses(w: &pod_actions_workflow::Workflow, base: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for (_, s) in local_steps(w, base)? {
        let Some(Kind::String(u)) = s.get("uses").map(|n| &n.kind) else { continue };
        // The broker resolves `actions/checkout` to its stand-in (T-708).
        if !u.starts_with("./") && !u.starts_with("docker://") && !u.starts_with("actions/checkout@") && !out.contains(u)
        {
            out.push(u.clone());
        }
    }
    Ok(out)
}

/// A checkout step of a local action: the path of the action, the line of
/// the step in its `action.yml`, and its `with:`.
pub(super) type Checkout = (String, u32, Option<Node>);

/// Each `uses: actions/checkout@REF` step of the local actions of the steps
/// of `w` (T-713): the run pins the trees of their static values.
pub(super) fn local_checkouts(w: &pod_actions_workflow::Workflow, base: &Path) -> Result<Vec<Checkout>, String> {
    let checkout = |s: &Node| matches!(s.get("uses").map(|n| &n.kind), Some(Kind::String(u)) if u.starts_with("actions/checkout@"));
    Ok(local_steps(w, base)?
        .into_iter()
        .filter(|(_, s)| checkout(s))
        .map(|(path, s)| (path, s.mark.line, s.get("with").cloned()))
        .collect())
}

/// Each `OWNER/REPO@REF` of a checkout of another repository in the local
/// actions of `w`, with static values (T-713). An expression, or a value
/// that is not a repository, gets no tree: the stand-in fails its step
/// with the fix.
pub(super) fn local_trees(
    w: &pod_actions_workflow::Workflow,
    base: &Path,
    self_repo: &str,
) -> Result<Vec<String>, String> {
    let all = local_checkouts(w, base)?.into_iter();
    Ok(all.filter_map(|(_, _, with)| podrunner_local::store::checkout_target(with.as_ref(), self_repo).ok().flatten()).collect())
}

/// Each step of each local composite action that the steps of `w` name,
/// and of each local action that one names, with the path of its action.
fn local_steps(w: &pod_actions_workflow::Workflow, base: &Path) -> Result<Vec<(String, Node)>, String> {
    let mut queue: Vec<(String, usize)> = w
        .jobs
        .iter()
        .flat_map(|j| j.steps.iter().filter_map(|s| s.uses.clone()))
        .filter(|u| u.starts_with("./"))
        .map(|u| (u, 1))
        .collect();
    let (mut seen, mut out) = (BTreeSet::new(), Vec::new());
    while let Some((path, depth)) = queue.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        if depth > DEPTH {
            return Err(format!("`uses: {path}`: the local actions nest deeper than {DEPTH} levels"));
        }
        // A local action stays in the tree: no `..`, and no link that leads
        // out of it (#243).
        let dir = base.join(&path);
        if std::path::Path::new(&path).components().any(|c| c == std::path::Component::ParentDir) {
            return Err(format!("`uses: {path}`: a local action is a path in the tree, with no `..`"));
        }
        let root = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
        if let Ok(real) = dir.canonicalize()
            && !real.starts_with(&root)
        {
            return Err(format!("`uses: {path}`: a symbolic link leads out of the tree ({})", real.display()));
        }
        let inside = |f: &std::path::Path| f.canonicalize().is_ok_and(|r| r.starts_with(&root));
        let Some(text) = ["action.yml", "action.yaml"]
            .iter()
            .map(|f| dir.join(f))
            .find_map(|f| inside(&f).then(|| std::fs::read_to_string(&f).ok()).flatten())
        else {
            continue;
        };
        for s in composite_steps(&text).map_err(|e| format!("the action.yml of `uses: {path}`: {e}"))? {
            if let Some(Kind::String(u)) = s.get("uses").map(|n| &n.kind)
                && u.starts_with("./")
            {
                queue.push((u.clone(), depth + 1));
            }
            out.push((path.clone(), s));
        }
    }
    Ok(out)
}

/// Each workflow of the tree that a job of `w` calls (`jobs.ID.uses: ./PATH`),
/// and each that one calls, to 10 levels (#238): the lock pins their
/// actions and images before the first job, as those of `w`.
pub(super) fn local_calls(
    w: &pod_actions_workflow::Workflow,
    base: &Path,
) -> Result<Vec<pod_actions_workflow::Workflow>, String> {
    let root = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    let mut queue: Vec<(String, usize)> =
        w.jobs.iter().filter_map(|j| j.uses.clone()).filter(|u| u.starts_with("./")).map(|u| (u, 1)).collect();
    let (mut seen, mut out) = (BTreeSet::new(), Vec::new());
    while let Some((path, depth)) = queue.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        if depth > DEPTH {
            return Err(format!("`uses: {path}`: the called workflows nest deeper than {DEPTH} levels"));
        }
        let file = base.join(&path);
        if Path::new(&path).components().any(|c| c == std::path::Component::ParentDir)
            || file.canonicalize().is_ok_and(|r| !r.starts_with(&root))
        {
            return Err(format!("`uses: {path}`: a called workflow is a file in the tree"));
        }
        // A file that is not there fails its job at its start, as now.
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let called = pod_actions_workflow::parse(&text).map_err(|e| format!("`uses: {path}`: {e}"))?.workflow;
        queue.extend(
            called.jobs.iter().filter_map(|j| j.uses.clone()).filter(|u| u.starts_with("./")).map(|u| (u, depth + 1)),
        );
        out.push(called);
    }
    Ok(out)
}

/// The steps of a composite `action.yml`; none for another kind.
fn composite_steps(text: &str) -> Result<Vec<Node>, String> {
    let root = pod_actions_workflow::yaml::load(text, Limits::default()).map_err(|e| e.to_string())?;
    let Some(runs) = root.get("runs") else { return Ok(Vec::new()) };
    if !matches!(runs.get("using").map(|n| &n.kind), Some(Kind::String(u)) if u == "composite") {
        return Ok(Vec::new());
    }
    let Some(Kind::Seq(steps)) = runs.get("steps").map(|n| &n.kind) else { return Ok(Vec::new()) };
    Ok(steps.clone())
}

#[cfg(test)]
mod tests {
    /// T-346: a remote `uses:` of a local composite action, also of a
    /// nested local one, is pinned; a loop of local actions ends.
    #[test]
    fn the_uses_of_local_composite_actions() {
        let dir = std::env::temp_dir().join(format!("podrunner-composites-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let comp = |p: &str, steps: &str| {
            std::fs::create_dir_all(dir.join(p)).unwrap();
            std::fs::write(dir.join(p).join("action.yml"), format!("runs:\n  using: composite\n  steps:\n{steps}"))
                .unwrap();
        };
        comp(
            ".github/actions/a",
            "    - uses: actions/setup-node@v4\n    - uses: actions/checkout@v4\n    - uses: ./.github/actions/b\n",
        );
        comp(
            ".github/actions/b",
            "    - uses: owner/x/sub@v1\n    - uses: ./.github/actions/a\n    - uses: docker://alpine:3\n",
        );
        let w = pod_actions_workflow::parse(
            "on: push\njobs:\n  j:\n    runs-on: x\n    steps:\n      - uses: ./.github/actions/a\n      - uses: ./missing\n",
        )
        .unwrap()
        .workflow;
        let mut got = super::local_uses(&w, &dir).unwrap();
        got.sort();
        assert_eq!(got, ["actions/setup-node@v4", "owner/x/sub@v1"]);
        // #243: a path with `..` or a link out of the tree is refused.
        let out = dir.with_extension("outside");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("action.yml"), "runs:\n  using: composite\n  steps: [{uses: owner/y@v1}]\n").unwrap();
        let up = |u: &str| {
            let t = format!("on: push\njobs:\n  j:\n    runs-on: x\n    steps: [{{uses: '{u}'}}]\n");
            super::local_uses(&pod_actions_workflow::parse(&t).unwrap().workflow, &dir)
        };
        let name = out.file_name().unwrap().to_string_lossy().into_owned();
        assert!(up(&format!("./../{name}")).unwrap_err().contains("no `..`"));
        std::os::unix::fs::symlink(&out, dir.join("link")).unwrap();
        assert!(up("./link").unwrap_err().contains("leads out of the tree"));
        let _ = std::fs::remove_dir_all(&out);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-713: the static `repository` and `ref` of each checkout in a local
    /// action, also a nested one, give a tree to pin; an expression, the
    /// same repository (by any case) and a bad name give none.
    #[test]
    fn the_trees_of_the_checkouts_of_local_actions() {
        let dir = std::env::temp_dir().join(format!("podrunner-composite-trees-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let comp = |p: &str, steps: &str| {
            std::fs::create_dir_all(dir.join(p)).unwrap();
            std::fs::write(dir.join(p).join("action.yml"), format!("runs:\n  using: composite\n  steps:\n{steps}"))
                .unwrap();
        };
        let co = |with: &str| format!("    - uses: actions/checkout@v4\n      with: {{{with}}}\n");
        comp(
            "a",
            &[
                co("repository: o/second, ref: v1"),
                co("repository: '${{ inputs.repo }}'"),
                co("repository: o/third, ref: '${{ inputs.ref }}'"),
                co("repository: Me/Self, ref: v2"),
                co("repository: not-a-repo"),
                "    - uses: ./b\n".to_string(),
            ]
            .concat(),
        );
        comp("b", &[co("repository: o/fourth"), co("ref: v3"), "    - run: echo\n".to_string()].concat());
        let w = pod_actions_workflow::parse("on: push\njobs:\n  j:\n    runs-on: x\n    steps: [{uses: ./a}]\n")
            .unwrap()
            .workflow;
        let mut got = super::local_trees(&w, &dir, "me/self").unwrap();
        got.sort();
        assert_eq!(got, ["o/fourth@HEAD", "o/second@v1"]);
        let lines: Vec<(String, u32)> =
            super::local_checkouts(&w, &dir).unwrap().into_iter().map(|(p, l, _)| (p, l)).collect();
        assert_eq!(lines.len(), 7, "{lines:?}");
        assert!(lines.contains(&("./b".to_string(), 6)) && lines.contains(&("./a".to_string(), 4)), "{lines:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
