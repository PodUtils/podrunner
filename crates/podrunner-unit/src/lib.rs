//! The unit seam of podrunner (GitHub #16, `docs/contracts/unit-driver.md`).
//!
//! A unit is one isolated environment for one job. A driver makes units of
//! one kind (`fake`, `podvm`, `microvm`, `incus`; `docker` and `host` are
//! non-goals of v1, `docs/non-goals.md`). The core of
//! podrunner calls only the trait [`UnitDriver`]; the driver is the only
//! code that knows its tool (its argv, its files). [`conformance`] holds the
//! tests that each driver must pass.

pub mod confine;
pub mod conformance;
pub mod limits;
mod log;
pub mod mounts;
pub mod procs;
pub mod shim;
mod stub;
mod types;

pub use podrunner_schema::{Capability, Credential, UnitId};

/// The name of the egress socket in the socket directory of a unit
/// (`StartSpec::sockets`).
pub const EGRESS_SOCKET: &str = "egress.sock";

/// The variable of a held unit that names the port of its broker on the
/// loopback of the host: the driver lets only that loopback port through the
/// hold (T-319).
pub const LOOPBACK_ALLOW: &str = "PODRUNNER_LOOPBACK_ALLOW";
/// The name of the control socket in the socket directory of a unit.
pub const CONTROL_SOCKET: &str = "ctl.sock";
pub use log::{Cut, LogRead, UnitLog};
pub use stub::Stub;

/// The use of a unit that its kind measures (T-483).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnitUsage {
    /// The CPU time, in whole seconds rounded up.
    pub cpu_s: u64,
    /// The peak memory, in bytes.
    pub peak_memory_bytes: u64,
}
pub use types::{
    ContainerId, ContainerMount, ContainerSpec, ContainerState, Count, ExecSpec, Exit, Observation, Prepared, Residue,
    Signal, StartSpec, UnitCode, UnitError, parse_pinned,
};

use std::time::Duration;

/// The operations of the unit seam, in the order of the contract. A test
/// compares them with `docs/contracts/unit-driver.md`.
pub const OPERATIONS: [&str; 12] = [
    "probe",
    "prepare",
    "start",
    "signal",
    "wait",
    "destroy",
    "observe",
    "container_start",
    "container_exec",
    "container_logs",
    "container_inspect",
    "container_stop",
];

/// A driver of one unit kind. Each method is safe to call from more than one
/// thread; the core calls the methods of one unit in order.
pub trait UnitDriver: Send + Sync {
    /// The unit kind, as in the capability record.
    fn kind(&self) -> &str;

    /// The capability record of this kind on this host. Two calls give the
    /// same record. A field that the driver cannot prove is `unknown`.
    fn probe(&self) -> Result<Capability, UnitError>;

    /// Where the unit sees its socket directory (`StartSpec::sockets`):
    /// the egress socket, the control socket and the hook relay. `None`:
    /// the unit reaches the host paths of the sockets.
    fn socket_mount(&self) -> Option<&str> {
        None
    }

    /// Make an image ready. `reference` names a digest (`NAME@sha256:HEX`);
    /// a tag is refused (`payload-not-pinned`). A `platform` other than the
    /// host's is refused (`arch-mismatch`) unless `emulation` is true and
    /// the driver can emulate it.
    fn prepare(&self, reference: &str, platform: &str, emulation: bool) -> Result<Prepared, UnitError>;

    /// Start the unit: a fresh root and store, the credential files in the
    /// runner root at mode 0600, and the top process. No credential goes
    /// into argv or the environment. A unit id that is in use is refused.
    fn start(&self, spec: &StartSpec) -> Result<(), UnitError>;

    /// Send a signal to each process of the unit (the whole tree).
    fn signal(&self, unit: &UnitId, signal: Signal) -> Result<(), UnitError>;

    /// Wait up to `timeout` for the end of the top process. `None`: it still
    /// runs.
    fn wait(&self, unit: &UnitId, timeout: Duration) -> Result<Option<Exit>, UnitError>;

    /// End each process and remove each file of the unit, then count what
    /// is left. A second call on the same unit succeeds and counts again.
    fn destroy(&self, unit: &UnitId) -> Result<Residue, UnitError>;

    /// The paths and the processes of a running unit, for the conformance
    /// tests and for `status`. A driver that cannot see a field leaves it
    /// empty.
    fn observe(&self, unit: &UnitId) -> Result<Observation, UnitError>;

    /// The CPU time and the peak memory of a unit whose processes are not
    /// on the host (an incus instance, T-483), before `destroy`. A kind
    /// whose processes slots samples on the host gives none.
    fn usage(&self, _unit: &UnitId) -> Option<UnitUsage> {
        None
    }

    /// The count of kills of the OOM killer in a unit, before `destroy`
    /// (T-515): a job that failed after one ends with `limit`. None: the
    /// kind does not know.
    fn oom_kills(&self, _unit: &UnitId) -> Option<u64> {
        None
    }

    /// Start a container of the job in the unit (the container hooks).
    fn container_start(&self, unit: &UnitId, spec: &ContainerSpec) -> Result<ContainerId, UnitError>;

    /// Run a command in a container of the unit. `stdin` is refused.
    fn container_exec(&self, unit: &UnitId, container: &ContainerId, spec: &ExecSpec) -> Result<Exit, UnitError>;

    /// The output of a container so far.
    fn container_logs(&self, unit: &UnitId, container: &ContainerId) -> Result<Vec<u8>, UnitError>;

    /// The state of a container.
    fn container_inspect(&self, unit: &UnitId, container: &ContainerId) -> Result<ContainerState, UnitError>;

    /// Stop a container and remove it.
    fn container_stop(&self, unit: &UnitId, container: &ContainerId) -> Result<(), UnitError>;

    /// The output of the payload of a unit so far, from its start, as a
    /// stream, before `destroy`: it tells why a job gave no result. The
    /// core reads it with a bound (`UnitLog::read_bounded`, T-570). A
    /// driver that keeps none refuses.
    fn unit_log(&self, unit: &UnitId) -> Result<UnitLog, UnitError> {
        Err(UnitError::new(UnitCode::Unsupported, format!("{}: no output of {unit}", self.kind())))
    }

    /// All of [`UnitDriver::unit_log`], with no bound: for a test or a
    /// person.
    fn unit_logs(&self, unit: &UnitId) -> Result<Vec<u8>, UnitError> {
        self.unit_log(unit)?.read_all()
    }

    /// A command that runs `argv` in the running unit for a person (a shell
    /// in a paused job, T-150), under the egress hold of the unit; the
    /// caller gives it its stdio. With `tty` it asks for a terminal. A
    /// driver that cannot enter a unit refuses.
    fn enter(&self, unit: &UnitId, argv: &[String], tty: bool) -> Result<std::process::Command, UnitError> {
        let _ = (argv, tty);
        Err(UnitError::new(UnitCode::Unsupported, format!("{}: no command enters {unit}", self.kind())))
    }
}

#[cfg(test)]
mod tests;
