//! The trees of the stand-in of `actions/checkout` (T-092, T-640): the
//! tarball of the local tree of `--checkout`, and for each `ref:` of a
//! checkout of the same repository the tarball of its commit in the local
//! git, before the first job. The pins of the run name each ref
//! (`podrunner_local::tree::refs::key`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use podrunner_local::tree::refs::{self, Ask};

use super::Args;

/// One line for each key of `with:` that the stand-in does not take
/// (T-643), once for a run: the key, its first line, and what the job gets.
fn ignored_notes<'a>(ws: impl IntoIterator<Item = &'a pod_actions_workflow::Workflow>) -> Vec<String> {
    podrunner_local::tree::ignored::ignored_in(ws)
        .into_iter()
        .map(|(line, key, why)| format!("podrunner: the checkout at line {line}: `{key}` is ignored; {why}"))
        .collect()
}

/// The tarball of the local tree, or `None` with no `--checkout`; each ref
/// of a checkout of the same repository goes into `pins`. A line on stderr
/// names each ref that has no commit (its step fails) and each expression
/// (the local tree); a dry run also names each commit.
pub(super) fn trees(
    a: &Args,
    workflow: &pod_actions_workflow::Workflow,
    self_repo: &str,
    scratch: &Path,
    pins: &mut BTreeMap<String, String>,
) -> Result<Option<PathBuf>, String> {
    let called = a.checkout.as_deref().map(|d| super::composites::local_calls(workflow, d).unwrap_or_default());
    for note in ignored_notes(std::iter::once(workflow).chain(called.iter().flatten())) {
        eprintln!("{note}");
    }
    let Some(dir) = &a.checkout else { return Ok(None) };
    let o = podrunner_local::tree::Options {
        head: a.checkout_head,
        with_git: a.with_git || (!a.no_git && podrunner_local::tree::is_git_top(dir)),
        exclude: super::run_files(a, dir),
    };
    let t = podrunner_local::tree::tarball(dir, scratch, o.clone())
        .map_err(|e| format!("--checkout {}: {e}", dir.display()))?;
    // Each checkout of each job downloads the tree: a guest takes it
    // through its channel, which is slow for a large tree.
    let mib = std::fs::metadata(&t).map(|m| m.len() >> 20).unwrap_or(0);
    if mib >= 100 {
        eprintln!(
            "podrunner: the tree of --checkout is {mib} MiB; each checkout of a job downloads it: ignore its large files in .gitignore, or give --checkout-head"
        );
    }
    let called = called.unwrap_or_default();
    let mut asks: Vec<(u32, Ask)> =
        std::iter::once(workflow).chain(&called).flat_map(|w| refs::asks(w, self_repo)).collect();
    asks.dedup();
    let head = refs::head(dir);
    for (line, ask) in asks {
        let r = match ask {
            Ask::Ref(r) => r,
            Ask::Expression(e) => {
                eprintln!(
                    "podrunner: the checkout at line {line}: `ref: {e}` is an expression; the stand-in gives the local tree"
                );
                continue;
            }
            Ask::Bad(why) => {
                eprintln!("podrunner: the checkout at line {line}: {why}; the step fails");
                continue;
            }
            Ask::Default => continue,
        };
        let key = refs::key(&r);
        if pins.contains_key(&key) {
            continue;
        }
        let value = match refs::resolve(dir, &r) {
            Ok(sha) if head.as_deref() == Some(sha.as_str()) => {
                if a.dry_run {
                    eprintln!(
                        "podrunner: the checkout at line {line}: `ref: {r}` is the commit of HEAD: the local tree"
                    );
                }
                String::new()
            }
            Ok(sha) => {
                refs::tarball(dir, scratch, &sha, &o).map_err(|e| format!("--checkout {}: {e}", dir.display()))?;
                if a.dry_run {
                    eprintln!(
                        "podrunner: the checkout at line {line}: `ref: {r}` gives the commit {sha} of the local git"
                    );
                }
                sha
            }
            Err(why) => {
                eprintln!("podrunner: the checkout at line {line}: {why}; the step fails");
                format!("!{why}")
            }
        };
        pins.insert(key, value);
    }
    Ok(Some(t))
}

#[cfg(test)]
mod tests {
    use pod_actions_workflow::Workflow;

    fn notes(with: &str) -> Vec<String> {
        let text = format!(
            "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n        with:\n          {with}\n      - uses: actions/checkout@v4\n        with:\n          {with}\n"
        );
        let w = pod_actions_workflow::parse(&text).unwrap().workflow;
        super::ignored_notes(std::iter::once::<&Workflow>(&w))
    }

    #[test]
    fn each_key_that_the_stand_in_ignores_is_named_once() {
        for key in [
            "fetch-depth: 0", "filter: blob:none", "sparse-checkout: src", "sparse-checkout-cone-mode: false",
            "clean: false", "lfs: true", "token: x", "ssh-key: x", "persist-credentials: false",
        ] {
            let n = notes(key);
            let name = key.split(':').next().unwrap();
            assert_eq!(n.len(), 1, "{key}: two steps, one line: {n:?}");
            assert!(n[0].contains(&format!("`{name}` is ignored")) && n[0].contains("line 6"), "{n:?}");
        }
        // The keys that it takes, and a step with none, have no line; a
        // value is never in the line.
        assert!(notes("path: x").is_empty() && notes("submodules: true").is_empty());
        assert!(notes("ref: main").is_empty() && notes("repository: o/r").is_empty());
        assert!(!notes("token: s3cr3t").concat().contains("s3cr3t"));
    }
}
