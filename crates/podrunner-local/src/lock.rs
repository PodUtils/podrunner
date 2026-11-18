//! The lock file of the actions and the images of a workflow (T-118,
//! T-130): the crate `pod-actions-store` of podcrates (GitHub #94), which
//! holds the lock that podrunner wrote first. Its tests are in the crate.

pub use pod_actions_store::lock::*;

/// The name of the lock file, beside the workflow.
pub const LOCK_FILE: &str = "podrunner-actions.lock";

#[cfg(test)]
mod tests {
    use super::*;

    /// GitHub #284: a lock with a digest in uppercase hex gives the digest
    /// that the store computes for the same bytes, so the fetch matches.
    #[test]
    fn an_uppercase_digest_matches_the_store() {
        let computed = pod_actions_store::store::digest(b"the tar of an action");
        let line = format!(
            "actions/checkout@v4 {} {}\n",
            "c".repeat(40),
            computed.to_uppercase().replace("SHA256:", "sha256:")
        );
        let lock = Lock::parse(&line).unwrap();
        assert_eq!(lock.pins["actions/checkout@v4"].digest, computed);
    }
}
