//! The records that cross the seams of podrunner (GitHub #17). The job
//! offer, the capability record of a unit kind, the run report and the
//! public report, the ids and the refusal codes come from `pod-job` of
//! podcrates (ruling Q-A8: the JSON Schemas live in podcrates). The
//! credential of a unit stays in podrunner.

pub use pod_job::{
    ARCHES, Assigned, Boundary, Cache, CacheMode, Capability, ContainerRoute, CredentialStore, EgressHold, ExitReason,
    Fact, IdError, JOB_ID_MAX, JobId, JobOffer, LimitMode, Principal, PublicReport, RefusalCode, Residual, RunReport,
    SCHEMA, SecretDelivery, Tier, TreeKill, UnitFields, UnitId, UnitIds, Verdict, capability, offer, report,
};
/// `status.v1` (podcrates `v0.73.0`, T-508): the status of a host.
pub use pod_job::{KindStatus, PinStatus, SourceStatus, Status};

mod secret;

pub use secret::{BINARY_MARK, Credential, CredentialFile, FileBytes, PlainName, Secret, binary_file};

#[cfg(test)]
mod tests {
    use super::*;

    /// Design unit test 12: a job id or a unit id that could climb out of a
    /// directory never parses, so it never becomes a path component.
    #[test]
    fn an_id_that_climbs_never_parses() {
        for bad in ["../x", "..", "a/b", "x/../y"] {
            assert!(JobId::new(bad).is_err(), "job id {bad:?}");
            assert!(UnitId::parse(bad).is_err(), "unit id {bad:?}");
            assert!(UnitId::parse(&format!("u-{bad}")).is_err(), "unit id u-{bad:?}");
        }
        assert!(JobId::new("1234.x_y-z").is_ok());
        assert!(UnitId::parse("u-12").is_ok());
    }
}
