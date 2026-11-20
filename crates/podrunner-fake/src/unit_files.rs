//! The files of a unit of the fake kind that a test reads: the output of
//! its payload, and the OOM kills that a payload says it had (T-515).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use podrunner_schema::UnitId;
use podrunner_unit::{UnitCode, UnitError};

/// The file of the output of the payload, in the root of a unit.
pub(crate) const UNIT_LOG: &str = ".unit.log";

/// A payload of a test writes a count here to stand for kills of the OOM
/// killer in its unit.
pub const OOM_KILLS: &str = ".oom_kills";

/// The root of the running unit `unit` of `units`.
pub(crate) fn root_of<T>(
    units: &HashMap<UnitId, T>,
    unit: &UnitId,
    root: impl Fn(&T) -> PathBuf,
) -> Result<PathBuf, UnitError> {
    units.get(unit).map(root).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))
}

/// The count of `OOM_KILLS` of the unit at `root`, if its payload wrote one.
pub(crate) fn oom_kills(root: &Path) -> Option<u64> {
    std::fs::read_to_string(root.join(OOM_KILLS)).ok()?.trim().parse().ok()
}
