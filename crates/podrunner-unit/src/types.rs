//! The values of the unit seam.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use podrunner_schema::{Credential, UnitId};
use serde::{Deserialize, Serialize};

/// The class of a failure of a driver. The names are the refusal codes of
/// GitHub #21 where one fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnitCode {
    /// `prepare` got a tag, or a reference with no digest.
    PayloadNotPinned,
    /// A platform that the host cannot run.
    ArchMismatch,
    /// The kind has no container route.
    ContainerNotSupported,
    /// A container option that the kind refuses (`stdin`, a workdir in a
    /// volume).
    ContainerOptionRefused,
    /// The unit did not start.
    UnitStartFailed,
    /// The unit id is in use.
    UnitInUse,
    /// No unit or container with this id.
    UnknownUnit,
    /// The driver does not do this operation.
    Unsupported,
    /// A fault of the host (a file, a process).
    Host,
}

impl UnitCode {
    pub fn as_str(self) -> &'static str {
        match self {
            UnitCode::PayloadNotPinned => "payload-not-pinned",
            UnitCode::ArchMismatch => "arch-mismatch",
            UnitCode::ContainerNotSupported => "container-not-supported",
            UnitCode::ContainerOptionRefused => "container-option-refused",
            UnitCode::UnitStartFailed => "unit-start-failed",
            UnitCode::UnitInUse => "unit-in-use",
            UnitCode::UnknownUnit => "unknown-unit",
            UnitCode::Unsupported => "unsupported",
            UnitCode::Host => "host",
        }
    }
}

/// A failure of a driver: a code and a detail with no credential in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitError {
    pub code: UnitCode,
    pub detail: String,
}

impl UnitError {
    pub fn new(code: UnitCode, detail: impl Into<String>) -> UnitError {
        UnitError { code, detail: detail.into() }
    }
}

impl fmt::Display for UnitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.detail)
    }
}

impl std::error::Error for UnitError {}

/// Split `NAME@sha256:HEX` into the name and the digest. A tag, a short or
/// an upper-case digest is `payload-not-pinned`.
pub fn parse_pinned(reference: &str) -> Result<(&str, &str), UnitError> {
    let refuse = || UnitError::new(UnitCode::PayloadNotPinned, format!("not NAME@sha256:HEX: {reference:?}"));
    let (name, digest) = reference.rsplit_once('@').ok_or_else(refuse)?;
    let hex = digest.strip_prefix("sha256:").ok_or_else(refuse)?;
    if name.is_empty() || hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err(refuse());
    }
    Ok((name, digest))
}

/// An image that `prepare` made ready.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prepared {
    /// `sha256:HEX`.
    pub digest: String,
    pub platform: String,
}

/// What `start` needs.
#[derive(Debug, Clone)]
pub struct StartSpec {
    pub unit: UnitId,
    pub image: Prepared,
    /// The top process, inside the unit.
    pub argv: Vec<String>,
    /// The environment of the top process. Never a credential.
    pub env: BTreeMap<String, String>,
    /// The files for the runner root, mode 0600.
    pub credential: Credential,
    /// The socket directory of this unit on the host, for a driver whose
    /// `socket_mount` names where the unit sees it.
    pub sockets: Option<std::path::PathBuf>,
}

/// The signals that the core sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Signal {
    Term,
    Kill,
}

/// The end of the top process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Exit {
    Code(i32),
    Signal(i32),
}

/// A count that the driver measured, or `unknown`. Placement reads
/// `unknown` as more than 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Count {
    Known(u64),
    Unknown,
}

impl Count {
    pub fn is_zero(self) -> bool {
        self == Count::Known(0)
    }
}

/// What `destroy` left behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Residue {
    pub processes: Count,
    pub files: Count,
}

impl Residue {
    pub const NONE: Residue = Residue { processes: Count::Known(0), files: Count::Known(0) };

    pub fn is_clean(&self) -> bool {
        self.processes.is_zero() && self.files.is_zero()
    }
}

/// The paths and the processes of a running unit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Observation {
    /// The root of the unit on the host.
    pub unit_root: Option<PathBuf>,
    /// The directory of the credential files on the host.
    pub runner_root: Option<PathBuf>,
    /// The host process ids of the unit.
    pub pids: Vec<u32>,
}

/// The id of a container in a unit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContainerId(pub String);

/// A directory of the unit in a container. The hook checks (GitHub #35)
/// give only sources under the runner directory of the unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerMount {
    /// A path in the unit root.
    pub source: std::path::PathBuf,
    /// A path in the container.
    pub target: String,
    pub read_only: bool,
}

/// A container of a job.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerSpec {
    /// A pinned reference (`NAME@sha256:HEX`).
    pub image: String,
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub workdir: Option<String>,
    pub mounts: Vec<ContainerMount>,
    /// The checked `createOptions` (`--cpus 2`); a driver applies each or
    /// refuses the start with `container-option-refused`.
    pub options: Vec<(String, String)>,
}

/// A command in a container.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecSpec {
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub workdir: Option<String>,
    pub stdin: bool,
    /// A host file for the output (stdout and stderr) of the command: the
    /// hook relay gives it to the runner as the output of a step.
    pub log: Option<std::path::PathBuf>,
}

/// The state of a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerState {
    pub running: bool,
    pub exit: Option<Exit>,
}
