//! The tree of a run with no `--checkout` (T-203, T-336, T-337): the top of
//! the git tree of the current directory, else the project whose
//! `.github/workflows` holds the file. A called workflow of `./PATH` comes
//! from the same tree, and its `actions/checkout` needs it too, as GitHub
//! checks out the repository of the caller in a called job (T-421).

use std::path::{Path, PathBuf};

use super::Args;

/// The top of the git tree that holds `dir`.
fn toplevel(dir: &std::path::Path) -> Option<PathBuf> {
    let out =
        std::process::Command::new("git").arg("-C").arg(dir).args(["rev-parse", "--show-toplevel"]).output().ok()?;
    let top = String::from_utf8(out.stdout).ok()?;
    (out.status.success() && !top.trim().is_empty()).then(|| PathBuf::from(top.trim()))
}

/// The refusal of a workflow file whose symbolic link leads out of its
/// project: the run would read, and check out, another project (T-340).
pub(in crate::run_local) fn link_out(file: &std::path::Path) -> Option<String> {
    if !std::fs::symlink_metadata(file).is_ok_and(|m| m.file_type().is_symlink()) {
        return None;
    }
    let dir =
        std::fs::canonicalize(file.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(".".as_ref())).ok()?;
    let root = project_of(file).or_else(|| toplevel(&dir)).unwrap_or(dir);
    let real = std::fs::canonicalize(file).unwrap_or_else(|_| std::fs::read_link(file).unwrap_or_default());
    (!real.starts_with(&root)).then(|| {
        format!(
            "{}: the symbolic link leads out of the project {}, to {}",
            file.display(),
            root.display(),
            real.display()
        )
    })
}

/// The project of a workflow file at `PROJECT/.github/workflows/FILE`; the
/// file itself is not followed.
fn project_of(file: &std::path::Path) -> Option<PathBuf> {
    let dir = file.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(".".as_ref());
    let file = std::fs::canonicalize(dir).ok()?.join(file.file_name()?);
    let workflows = file.parent().filter(|d| d.file_name().is_some_and(|n| n == "workflows"))?;
    let github = workflows.parent().filter(|d| d.file_name().is_some_and(|n| n == ".github"))?;
    github.parent().map(std::path::Path::to_path_buf)
}

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
    // GitHub matches the owner and the name of an action with no case (T-726).
    if text.to_ascii_lowercase().contains("actions/checkout@") {
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
    // A view of the file (`--list`, `--graph`, `--validate`) checks out
    // nothing, so it needs no tree (T-634).
    if a.checkout.is_some() || a.list || a.graph || a.validate {
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
        // T-726: by any case of the owner and the name.
        let step = "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{uses: Actions/Checkout@v4}]\n";
        assert_eq!(super::uses_checkout(step, None, 0), Some(true));
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod project_tests {
    /// T-336: a workflow file of `.github/workflows` names its project.
    #[test]
    fn the_project_of_a_workflow_file() {
        let dir = std::env::temp_dir().join(format!("podrunner-project-of-{}", std::process::id()));
        let wf = dir.join("p/.github/workflows/c.yml");
        std::fs::create_dir_all(wf.parent().unwrap()).unwrap();
        std::fs::write(&wf, "on: push\n").unwrap();
        std::fs::write(dir.join("c.yml"), "on: push\n").unwrap();
        let canon = std::fs::canonicalize(dir.join("p")).unwrap();
        assert_eq!(super::project_of(&wf), Some(canon));
        assert_eq!(super::project_of(&dir.join("c.yml")), None, "not in .github/workflows");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
