//! The trees of the stand-in of `actions/checkout` (T-092, T-640): the
//! tarball of the local tree of `--checkout`, and for each `ref:` of a
//! checkout of the same repository the tarball of its commit in the local
//! git, before the first job. The pins of the run name each ref
//! (`podrunner_local::tree::refs::key`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use podrunner_local::tree::refs::{self, Ask};

use super::Args;

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
    let called = super::composites::local_calls(workflow, dir).unwrap_or_default();
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
