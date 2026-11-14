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
