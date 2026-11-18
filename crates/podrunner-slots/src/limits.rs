//! The limits of a unit kind from its configuration (G-12, T-162).

use crate::config::UnitConfig;

/// The limits of `[unit.KIND] mem`, `cpus` and `pids` (T-162).
pub fn limits_of(u: &UnitConfig) -> Result<podrunner_unit::limits::Limits, String> {
    let memory_bytes =
        u.mem.as_deref().map(podrunner_unit::limits::size_bytes).transpose().map_err(|e| format!("mem: {e}"))?;
    if u.pids == Some(0) {
        return Err("pids: 0".into());
    }
    Ok(podrunner_unit::limits::Limits { memory_bytes, cpus: u.cpus, pids: u.pids, disk_bytes: None })
}
