//! The actions by tag and the trees of other repositories, pinned by the
//! lock and kept in a store of tarballs (T-118, T-133, T-139, T-154): the
//! crate `pod-actions-store` of podcrates (GitHub #94). Its tests are in
//! the crate; the live test is `tests/store.rs`.

pub use pod_actions_store::store::*;
pub use pod_actions_store::{Error, Git};

pub mod checkouts;

pub use podrunner_broker_fake::actions::is_checkout;

/// The prefix of `uses: $/PATH` (T-729): an action of the repository of the
/// workflow at its commit (`SelfRepositoryAlias` of the runner).
pub const SELF_PREFIX: &str = "$/";

/// `uses:` names an action of the repository of the workflow (`$/PATH`).
pub fn is_self(uses: &str) -> bool {
    uses.starts_with(SELF_PREFIX)
}

/// The path of `uses: $/PATH` in its repository, with no `/` at the start
/// or the end, as the runner reads it; `None` for another `uses:`. A
/// `@REF` is refused, as the runner refuses it.
pub fn self_path(uses: &str) -> Option<Result<&str, String>> {
    let path = uses.strip_prefix(SELF_PREFIX)?.trim_matches('/');
    Some(match path.contains('@') {
        true => Err(format!("`uses: {uses}`: `$/PATH` takes no @REF: it is the commit of the workflow")),
        false => Ok(path),
    })
}

/// `uses: $/PATH` as `REPO/PATH@COMMIT`: the action of the repository
/// `repo` at `commit`, as the runner resolves it.
pub fn self_uses(uses: &str, repo: &str, commit: &str) -> Option<Result<String, String>> {
    self_path(uses).map(|p| {
        p.map(|p| match p {
            "" => format!("{repo}@{commit}"),
            p => format!("{repo}/{p}@{commit}"),
        })
    })
}

/// `uses: $/PATH` of a step of the action `parent` (`./P`, or
/// `OWNER/REPO[/P]@REF`): `./PATH` of the tree, or `OWNER/REPO/PATH@REF`,
/// as the runner resolves it. `None` for another `uses:` or a bad one.
pub fn self_of(uses: &str, parent: &str) -> Option<String> {
    let path = self_path(uses)?.ok()?;
    if parent.starts_with("./") || is_self(parent) {
        return Some(format!("./{path}"));
    }
    let (left, r) = parent.rsplit_once('@')?;
    let repo: Vec<&str> = left.splitn(3, '/').take(2).collect();
    self_uses(uses, &repo.join("/"), r)?.ok()
}

/// Each `uses:` of a repository in the steps of the workflow, but the
/// checkout, which the local mode gives with its own stand-in (T-133), by
/// any case of its name (T-726).
pub fn workflow_uses(w: &pod_actions_workflow::Workflow) -> Vec<String> {
    let mut out = pod_actions_store::workflow_uses(w, &["actions/checkout"]);
    out.retain(|u| !is_checkout(u));
    out
}

/// Check the repository of a remote `uses: OWNER/REPO[/PATH]@REF` (#261):
/// the owner and the repository are names of GitHub, and no part is `.`
/// or `..`, which a URL would resolve to another repository or path.
pub fn repo_ok(uses: &str) -> Result<(), String> {
    let target = uses.rsplit_once('@').map_or(uses, |(t, _)| t);
    let mut parts = target.split('/');
    let name = |p: &str| {
        !p.is_empty() && p != "." && p != ".." && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    let (owner, repo) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if !name(owner) || !name(repo) || parts.any(|p| p == "." || p == ".." || p.is_empty()) {
        return Err(format!("`uses: {uses}`: OWNER/REPO[/PATH] has a part that is empty, `.`, `..` or not a name"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// GitHub #251 (T-439): a checkout of another repository with an empty
    /// ref is `HEAD`; a full ref and a commit id go to the lookup as written.
    #[test]
    fn the_ref_of_a_checkout_of_another_repository() {
        let sha = "4f0c1d2a".repeat(5);
        let text = [
            "on: push",
            "jobs:",
            "  a:",
            "    runs-on: ubuntu-latest",
            "    steps:",
            "      - uses: actions/checkout@v4",
            "        with: { repository: o/empty, ref: '' }",
            "      - uses: actions/checkout@v4",
            "        with: { repository: o/full, ref: refs/tags/v1 }",
            "      - uses: actions/checkout@v4",
            &format!("        with: {{ repository: o/sha, ref: {sha} }}"),
        ]
        .join("\n");
        let w = pod_actions_workflow::parse(&text).unwrap().workflow;
        let trees = super::workflow_trees(&w, "o/self").unwrap();
        assert_eq!(trees, ["o/empty@HEAD".to_string(), "o/full@refs/tags/v1".into(), format!("o/sha@{sha}")]);
    }

    /// T-726: the checkout by any case of the owner and the name is no
    /// action of the store.
    #[test]
    fn the_checkout_by_any_case_is_no_action_of_the_store() {
        let text = "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - uses: Actions/Checkout@v4\n      \
                    - uses: ACTIONS/checkout@V4\n      - uses: o/r@v1\n";
        let w = pod_actions_workflow::parse(text).unwrap().workflow;
        assert_eq!(super::workflow_uses(&w), ["o/r@v1"]);
    }

    /// T-729: `$/PATH` as the runner resolves it: the repository of the
    /// workflow, or of the parent action, at its commit; `@REF` is refused.
    #[test]
    fn a_uses_of_the_repository_of_the_workflow() {
        let sha = "a".repeat(40);
        assert_eq!(super::self_uses("$/x/y", "o/r", &sha), Some(Ok(format!("o/r/x/y@{sha}"))));
        assert_eq!(super::self_uses("$//x/", "o/r", &sha), Some(Ok(format!("o/r/x@{sha}"))));
        assert_eq!(super::self_uses("$/", "o/r", &sha), Some(Ok(format!("o/r@{sha}"))));
        assert!(super::self_uses("$/x@v1", "o/r", &sha).unwrap().unwrap_err().contains("takes no @REF"));
        for other in ["./x", "o/r@v1", "docker://a", "$x", "x$/y"] {
            assert_eq!(super::self_uses(other, "o/r", &sha), None, "{other}");
        }
        assert_eq!(super::self_of("$/b", "./a").as_deref(), Some("./b"));
        assert_eq!(super::self_of("$/b", "$/a").as_deref(), Some("./b"));
        assert_eq!(super::self_of("$/b/c", "o/r/a@v1").as_deref(), Some("o/r/b/c@v1"));
        assert_eq!(super::self_of("$/b", "o/r@v1").as_deref(), Some("o/r/b@v1"));
        assert_eq!(super::self_of("$/b@v2", "o/r@v1"), None);
        assert_eq!(super::self_of("o/x@v1", "o/r@v1"), None);
    }

    /// #261: a part of `.` or `..`, or one that is not a name, is refused.
    #[test]
    fn the_repository_of_a_uses_is_a_name() {
        for ok in ["actions/setup-node@v4", "o/r/sub/dir@v1", "o.x/r_y-z@main", "o/r@4f0c1d2"] {
            assert!(super::repo_ok(ok).is_ok(), "{ok}");
        }
        for bad in [
            "../x@v1",
            "o/..@v1",
            "./x@v1",
            "o/r/../../other@v1",
            "o//r@v1",
            "o@v1",
            "o/r/@v1",
            "o/r x@v1",
            "o/r%2e@v1",
        ] {
            assert!(super::repo_ok(bad).unwrap_err().contains("not a name"), "{bad}");
        }
    }
}
