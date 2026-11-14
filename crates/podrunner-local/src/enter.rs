//! The running unit of each job, for a person who enters it (T-150): a
//! paused job gets a shell in its unit. One table for the process: the
//! executors of each slot fill it, and the pause reads it.

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::{Arc, Mutex};

use podrunner_unit::{UnitDriver, UnitId};

type Units = BTreeMap<String, (Arc<dyn UnitDriver>, UnitId)>;

static UNITS: Mutex<Units> = Mutex::new(BTreeMap::new());

/// The unit of `job` runs.
pub fn register(job: &str, driver: Arc<dyn UnitDriver>, unit: &UnitId) {
    UNITS.lock().expect("the units").insert(job.to_string(), (driver, unit.clone()));
}

/// The unit of `job` goes.
pub fn forget(job: &str) {
    UNITS.lock().expect("the units").remove(job);
}

/// The argv of a shell: bash where the image has it, else sh.
pub fn shell() -> Vec<String> {
    let script =
        "cd \"${GITHUB_WORKSPACE:-$HOME}\" 2>/dev/null; command -v bash >/dev/null && exec bash -i; exec sh -i";
    ["/bin/sh", "-c", script].map(String::from).to_vec()
}

/// A command that runs `argv` in the unit of `job`.
pub fn command(job: &str, argv: &[String], tty: bool) -> Result<Command, String> {
    let units = UNITS.lock().expect("the units");
    let (driver, unit) = units.get(job).ok_or_else(|| format!("{job} has no running unit"))?;
    driver.enter(unit, argv, tty).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_with_no_unit_has_no_shell() {
        let e = command("no-such.0", &shell(), false).unwrap_err();
        assert!(e.contains("no running unit"), "{e}");
        forget("no-such.0");
        assert!(shell()[2].contains("exec sh -i"));
    }
}
