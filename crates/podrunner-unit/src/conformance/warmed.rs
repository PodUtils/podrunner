//! The tests 1 to 32 through warm units (T-487): a fixture whose driver
//! makes each unit with `warm`, waits for `warm_ready`, then gives it the
//! rest of the start with `claim`. A unit of a warm pool then keeps each
//! rule of a unit of `start`.

use std::path::PathBuf;
use std::time::Duration;

use podrunner_schema::UnitId;

use super::{Behaviour, Fixture};
use crate::limits::Limits;
use crate::warm::{ClaimSpec, WarmSpec, split, wait_ready};
use crate::{
    Capability, ContainerId, ContainerSpec, ContainerState, ExecSpec, Exit, Observation, Prepared, Residue, Signal,
    StartSpec, UnitDriver, UnitError, UnitLog, UnitUsage,
};

/// A driver whose `start` is `warm`, `warm_ready` and `claim` of `inner`.
pub struct Warmed<'a> {
    pub inner: &'a dyn UnitDriver,
    /// The time that a unit may take to be ready.
    pub ready: Duration,
}

impl UnitDriver for Warmed<'_> {
    fn kind(&self) -> &str {
        self.inner.kind()
    }
    fn probe(&self) -> Result<Capability, UnitError> {
        self.inner.probe()
    }
    fn socket_mount(&self) -> Option<&str> {
        self.inner.socket_mount()
    }
    fn host_mounts(&self) -> bool {
        false
    }
    fn credential_dir(&self, unit: &UnitId) -> Option<String> {
        self.inner.credential_dir(unit)
    }
    fn prepare(&self, reference: &str, platform: &str, emulation: bool) -> Result<Prepared, UnitError> {
        self.inner.prepare(reference, platform, emulation)
    }
    fn start(&self, spec: &StartSpec) -> Result<(), UnitError> {
        let (warm, claim) = split(spec)?;
        self.inner.warm(&warm)?;
        // A unit that failed after `warm` goes: the caller saw no start.
        let done = wait_ready(self.inner, &spec.unit, self.ready).and_then(|()| self.inner.claim(&spec.unit, &claim));
        if done.is_err() {
            let _ = self.inner.destroy(&spec.unit);
        }
        done
    }
    fn warm(&self, spec: &WarmSpec) -> Result<(), UnitError> {
        self.inner.warm(spec)
    }
    fn warm_ready(&self, unit: &UnitId) -> Result<bool, UnitError> {
        self.inner.warm_ready(unit)
    }
    fn claim(&self, unit: &UnitId, spec: &ClaimSpec) -> Result<(), UnitError> {
        self.inner.claim(unit, spec)
    }
    fn applied_limits(&self, job: &Limits) -> Limits {
        self.inner.applied_limits(job)
    }
    fn signal(&self, unit: &UnitId, signal: Signal) -> Result<(), UnitError> {
        self.inner.signal(unit, signal)
    }
    fn wait(&self, unit: &UnitId, timeout: Duration) -> Result<Option<Exit>, UnitError> {
        self.inner.wait(unit, timeout)
    }
    fn destroy(&self, unit: &UnitId) -> Result<Residue, UnitError> {
        self.inner.destroy(unit)
    }
    fn observe(&self, unit: &UnitId) -> Result<Observation, UnitError> {
        self.inner.observe(unit)
    }
    fn usage(&self, unit: &UnitId) -> Option<UnitUsage> {
        self.inner.usage(unit)
    }
    fn oom_kills(&self, unit: &UnitId) -> Option<u64> {
        self.inner.oom_kills(unit)
    }
    fn container_start(&self, unit: &UnitId, spec: &ContainerSpec) -> Result<ContainerId, UnitError> {
        self.inner.container_start(unit, spec)
    }
    fn container_exec(&self, unit: &UnitId, container: &ContainerId, spec: &ExecSpec) -> Result<Exit, UnitError> {
        self.inner.container_exec(unit, container, spec)
    }
    fn container_logs(&self, unit: &UnitId, container: &ContainerId) -> Result<Vec<u8>, UnitError> {
        self.inner.container_logs(unit, container)
    }
    fn container_inspect(&self, unit: &UnitId, container: &ContainerId) -> Result<ContainerState, UnitError> {
        self.inner.container_inspect(unit, container)
    }
    fn container_stop(&self, unit: &UnitId, container: &ContainerId) -> Result<(), UnitError> {
        self.inner.container_stop(unit, container)
    }
    fn unit_log(&self, unit: &UnitId) -> Result<UnitLog, UnitError> {
        self.inner.unit_log(unit)
    }
    fn enter(&self, unit: &UnitId, argv: &[String], tty: bool) -> Result<std::process::Command, UnitError> {
        self.inner.enter(unit, argv, tty)
    }
}

/// A fixture whose driver is [`Warmed`] over the driver of `inner`.
pub struct WarmFixture<'a> {
    inner: &'a dyn Fixture,
    driver: Warmed<'a>,
}

impl<'a> WarmFixture<'a> {
    pub fn new(inner: &'a dyn Fixture) -> WarmFixture<'a> {
        WarmFixture { inner, driver: Warmed { inner: inner.driver(), ready: super::warm::READY } }
    }
}

impl Fixture for WarmFixture<'_> {
    fn driver(&self) -> &dyn UnitDriver {
        &self.driver
    }
    fn image(&self) -> String {
        self.inner.image()
    }
    fn platform(&self) -> String {
        self.inner.platform()
    }
    fn foreign_platform(&self) -> String {
        self.inner.foreign_platform()
    }
    fn payload(&self, behaviour: Behaviour) -> Vec<String> {
        self.inner.payload(behaviour)
    }
    fn unit(&self) -> UnitId {
        self.inner.unit()
    }
    fn state_dir(&self) -> Option<PathBuf> {
        self.inner.state_dir()
    }
}
