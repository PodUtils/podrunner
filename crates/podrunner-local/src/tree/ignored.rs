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
        if !s.uses.as_deref().is_some_and(crate::store::is_checkout) {
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

#[cfg(test)]
mod tests {
    /// T-726: a checkout by any case of the owner and the name has its
    /// ignored keys named; another action has none.
    #[test]
    fn a_checkout_by_any_case_has_its_ignored_keys() {
        let text = "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - uses: Actions/Checkout@v4\n        \
                    with: {fetch-depth: 0}\n      - uses: ACTIONS/checkout@v4\n        with: {lfs: true}\n      \
                    - uses: o/checkout@v4\n        with: {clean: false}\n";
        let w = pod_actions_workflow::parse(text).unwrap().workflow;
        let keys: Vec<(u32, &str)> = super::ignored_in([&w]).into_iter().map(|(l, k, _)| (l, k)).collect();
        assert_eq!(keys, [(6, "fetch-depth"), (8, "lfs")]);
    }
}
