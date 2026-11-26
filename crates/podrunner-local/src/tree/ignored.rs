//! The keys of `with:` of `actions/checkout` that the stand-in does not
//! take (T-643): the stand-in gives the local tree, or a commit of the
//! local git, and does no fetch. The run names each key on stderr and the
//! display of the step lists them, so a job never gets another tree than
//! on GitHub with no word.

use pod_actions_workflow::{Kind, Step, Workflow};

/// Each ignored key, and what the job gets in its place.
const KEYS: [(&str, &str); 9] = [
    ("fetch-depth", "the tree has the whole `.git` with --with-git, else no `.git`: no history to fetch"),
    ("filter", "the stand-in fetches nothing: no partial clone"),
    ("sparse-checkout", "the job gets the whole tree, not the paths of the list"),
    ("sparse-checkout-cone-mode", "the job gets the whole tree, not a cone"),
    ("clean", "the stand-in extracts over the workspace and removes no file"),
    ("lfs", "the files are those of the local tree, as they are"),
    ("token", "the stand-in uses no credential"),
    ("ssh-key", "the stand-in uses no credential"),
    ("persist-credentials", "the stand-in stores no credential"),
];

/// Each key of `with:` of the checkout step `s` that the stand-in ignores,
/// in the order of the step, with what the job gets.
pub fn ignored(s: &Step) -> Vec<(&'static str, &'static str)> {
    let Some(Kind::Map(entries)) = s.with.as_ref().map(|w| &w.kind) else { return Vec::new() };
    let mut out = Vec::new();
    for (k, _) in entries {
        if let Some((name, why)) = KEYS.iter().find(|(n, _)| *n == k.name) {
            out.push((*name, *why));
        }
    }
    out
}

/// Each ignored key of each checkout step of `w`: the line of the first
/// step that has it, the key, and what the job gets. One entry for a key.
pub fn ignored_in<'a>(ws: impl IntoIterator<Item = &'a Workflow>) -> Vec<(u32, &'static str, &'static str)> {
    let mut out: Vec<(u32, &'static str, &'static str)> = Vec::new();
    for s in ws.into_iter().flat_map(|w| w.jobs.iter().flat_map(|j| &j.steps)) {
        if !s.uses.as_deref().is_some_and(|u| u.starts_with("actions/checkout@")) {
            continue;
        }
        for (k, why) in ignored(s) {
            if !out.iter().any(|(_, n, _)| *n == k) {
                out.push((s.mark.line, k, why));
            }
        }
    }
    out
}
