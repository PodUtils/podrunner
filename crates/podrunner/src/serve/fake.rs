//! The capability record of the `fake` kind (T-511).

use podrunner_schema::{
    Boundary, Capability, ContainerRoute, CredentialStore, EgressHold, Fact, LimitMode, Principal, SecretDelivery,
    TreeKill,
};

/// The record of the `fake` kind: no boundary, a shared principal, no hold.
/// It meets only the floor of `local`: a fake unit never runs a forge job
/// of `trusted` or `untrusted`, unless its table has a `claim`.
pub fn fake_capability() -> Capability {
    Capability {
        schema: podrunner_schema::SCHEMA,
        kind: "fake".into(),
        arch: std::env::consts::ARCH.into(),
        boundary: Boundary::None,
        boundary_proof: None,
        principal: Principal::Shared,
        container: ContainerRoute::None,
        net: "shared-host".into(),
        secret_delivery: vec![SecretDelivery::Files],
        credential_store: CredentialStore::PrivateDir,
        mount_modes: vec!["rw".into()],
        rootfs_fresh: Fact::Yes,
        tree_kill: TreeKill::Verified,
        limits: [("wall".to_string(), LimitMode::Enforced)].into(),
        egress_hold: EgressHold::None,
        kvm: Fact::Unknown,
    }
}

/// The record of the fake kind with the `claim` of its table (T-511): the
/// boundary of the claim and an enforced hold, for a test that drives a
/// forge source through `serve` (githome's G-40). The fake holds nothing.
pub fn fake_capability_of(u: &podrunner_slots::config::UnitConfig) -> Capability {
    let mut cap = fake_capability();
    if let Some(c) = u.claim {
        cap.boundary = c;
        cap.principal = Principal::Distinct;
        cap.egress_hold = EgressHold::Enforced;
        cap.net = "none".into();
    }
    cap
}
