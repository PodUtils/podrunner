//! A driver that refuses each operation. The conformance harness must fail
//! each of its tests on it: the plant of the harness itself.

use std::time::Duration;

use podrunner_schema::{Capability, UnitId};

use crate::{
    ContainerId, ContainerSpec, ContainerState, ExecSpec, Exit, Observation, Prepared, Residue, Signal, StartSpec,
    UnitCode, UnitDriver, UnitError,
};

/// The driver that does nothing.
#[derive(Debug, Default)]
pub struct Stub;

fn no<T>() -> Result<T, UnitError> {
    Err(UnitError::new(UnitCode::Unsupported, "the stub driver does nothing"))
}

impl UnitDriver for Stub {
    fn kind(&self) -> &str {
        "stub"
    }
    fn probe(&self) -> Result<Capability, UnitError> {
        no()
    }
    fn prepare(&self, _: &str, _: &str, _: bool) -> Result<Prepared, UnitError> {
        no()
    }
    fn start(&self, _: &StartSpec) -> Result<(), UnitError> {
        no()
    }
    fn signal(&self, _: &UnitId, _: Signal) -> Result<(), UnitError> {
        no()
    }
    fn wait(&self, _: &UnitId, _: Duration) -> Result<Option<Exit>, UnitError> {
        no()
    }
    fn destroy(&self, _: &UnitId) -> Result<Residue, UnitError> {
        no()
    }
    fn observe(&self, _: &UnitId) -> Result<Observation, UnitError> {
        no()
    }
    fn container_start(&self, _: &UnitId, _: &ContainerSpec) -> Result<ContainerId, UnitError> {
        no()
    }
    fn container_exec(&self, _: &UnitId, _: &ContainerId, _: &ExecSpec) -> Result<Exit, UnitError> {
        no()
    }
    fn container_logs(&self, _: &UnitId, _: &ContainerId) -> Result<Vec<u8>, UnitError> {
        no()
    }
    fn container_inspect(&self, _: &UnitId, _: &ContainerId) -> Result<ContainerState, UnitError> {
        no()
    }
    fn container_stop(&self, _: &UnitId, _: &ContainerId) -> Result<(), UnitError> {
        no()
    }
}
