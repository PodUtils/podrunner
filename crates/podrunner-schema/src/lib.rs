//! The records that cross the seams of podrunner (GitHub #17). The job
//! offer, the capability record of a unit kind, the run report and the
//! public report, the ids and the refusal codes come from `pod-job` of
//! podcrates (ruling Q-A8: the JSON Schemas live in podcrates). The
//! credential of a unit stays in podrunner.

pub use pod_job::{
    ARCHES, Boundary, Capability, ContainerRoute, CredentialStore, EgressHold, ExitReason, Fact, IdError, JOB_ID_MAX,
    JobId, JobOffer, LimitMode, Principal, PublicReport, RefusalCode, Residual, RunReport, SCHEMA, SecretDelivery,
    Tier, TreeKill, UnitFields, UnitId, UnitIds, capability, offer, report,
};

mod secret;

pub use secret::{Credential, CredentialFile, Secret};
