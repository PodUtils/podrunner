//! The actions by tag and the trees of other repositories, pinned by the
//! lock and kept in a store of tarballs (T-118, T-133, T-139, T-154): the
//! crate `pod-actions-store` of podcrates (GitHub #94). Its tests are in
//! the crate; the live test is `tests/store.rs`.

pub use pod_actions_store::store::*;
pub use pod_actions_store::{Error, Git};

/// Each `uses:` of a repository in the steps of the workflow, but the
/// checkout, which the local mode gives with its own stand-in (T-133).
pub fn workflow_uses(w: &pod_actions_workflow::Workflow) -> Vec<String> {
    pod_actions_store::workflow_uses(w, &["actions/checkout"])
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
