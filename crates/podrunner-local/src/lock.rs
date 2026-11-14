//! The lock file of the actions and the images of a workflow (T-118,
//! T-130): the crate `pod-actions-store` of podcrates (GitHub #94), which
//! holds the lock that podrunner wrote first. Its tests are in the crate.

pub use pod_actions_store::lock::*;

/// The name of the lock file, beside the workflow.
pub const LOCK_FILE: &str = "podrunner-actions.lock";
