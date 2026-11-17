//! The tree of a run with no `--checkout` (T-203, T-336, T-337): the top of
//! the git tree of the current directory, else the project whose
//! `.github/workflows` holds the file. A called workflow of `./PATH` comes
//! from the same tree, and its `actions/checkout` needs it too, as GitHub
//! checks out the repository of the caller in a called job (T-421).

use std::path::{Path, PathBuf};

use super::{Args, link_out, project_of, toplevel};

/// The tree of the project of `file`.
fn root_of(file: &Path) -> Option<PathBuf> {
    toplevel(&std::env::current_dir().ok()?).or_else(|| project_of(file))
}

/// The tree of the called workflows `./PATH`: `--checkout`, else the tree
/// of the project, else the current directory.
pub(in crate::run_local) fn calls_root(a: &Args) -> PathBuf {
    a.checkout.clone().or_else(|| root_of(&a.file)).unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

/// The use of `actions/checkout` by the workflow `text` or a workflow of
/// `./PATH` that it calls: `Some(true)` for a use; `Some(false)` for a call
/// of another repository, which is not read before its pin, and can use it.
fn uses_checkout(text: &str, root: Option<&Path>, depth: usize) -> Option<bool> {
    if text.contains("actions/checkout@") {
        return Some(true);
    }
    let p = pod_actions_workflow::parse(text).ok()?;
    let mut maybe = None;
    for uses in p.workflow.jobs.iter().filter_map(|j| j.uses.as_deref()) {
        let found = match uses.strip_prefix("./") {
            Some(path) if depth < 10 => root
                .and_then(|r| std::fs::read_to_string(r.join(path)).ok())
                .and_then(|t| uses_checkout(&t, root, depth + 1)),
            Some(_) => None,
            None => Some(false),
        };
        match found {
            Some(true) => return Some(true),
            Some(false) => maybe = Some(false),
            None => {}
        }
    }
    maybe
}

/// With no `--checkout`, a workflow that uses `actions/checkout@`, or calls
/// a workflow that uses it, takes the tree of the project. With no tree,
/// the run is refused with the fix: a checkout of nothing fails in the job.
pub(in crate::run_local) fn default_checkout(mut a: Args) -> Result<Args, String> {
    if let Some(e) = link_out(&a.file) {
        return Err(e);
    }
    if a.checkout.is_some() {
        return Ok(a);
    }
    let root = root_of(&a.file);
    let Some(sure) = std::fs::read_to_string(&a.file).ok().and_then(|t| uses_checkout(&t, root.as_deref(), 0)) else {
        return Ok(a);
    };
    let Some(root) = root else {
        if !sure {
            return Ok(a);
        }
        let cwd = std::env::current_dir().map_err(|e| format!("the current directory: {e}"))?;
        return Err(format!(
            "{}: the workflow uses actions/checkout, and {} is in no git tree: give --checkout DIR, the tree of the project",
            a.file.display(),
            cwd.display()
        ));
    };
    let why = if sure {
        "(or a workflow that it calls) uses actions/checkout"
    } else {
        "calls a workflow of another repository"
    };
    eprintln!("podrunner: the workflow {why}: it gets the tree of {} (--checkout DIR)", root.display());
    a.checkout = Some(root);
    Ok(a)
}

#[cfg(test)]
mod tests {
    /// T-421: a called workflow of `./PATH` that uses checkout counts, to
    /// each level; a call of another repository can use it.
    #[test]
    fn a_called_workflow_that_uses_checkout_counts() {
        let root = std::env::temp_dir().join(format!("podrunner-checkout-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".github/workflows")).unwrap();
        let w = |name: &str, text: &str| std::fs::write(root.join(".github/workflows").join(name), text).unwrap();
        w("b.yml", "on: workflow_call\njobs:\n  b:\n    uses: ./.github/workflows/c.yml\n");
        w("c.yml", "on: workflow_call\njobs:\n  c:\n    runs-on: x\n    steps: [{uses: actions/checkout@v4}]\n");
        w("plain.yml", "on: workflow_call\njobs:\n  p:\n    runs-on: x\n    steps: [{run: x}]\n");
        let call = |uses: &str| format!("on: push\njobs:\n  a:\n    uses: {uses}\n");
        let r = Some(root.as_path());
        assert_eq!(super::uses_checkout(&call("./.github/workflows/b.yml"), r, 0), Some(true));
        assert_eq!(super::uses_checkout(&call("./.github/workflows/plain.yml"), r, 0), None);
        assert_eq!(super::uses_checkout(&call("o/r/.github/workflows/x.yml@v1"), r, 0), Some(false));
        assert_eq!(super::uses_checkout(&call("./.github/workflows/b.yml"), None, 0), None, "no tree");
        let _ = std::fs::remove_dir_all(&root);
    }
}
