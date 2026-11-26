//! The OOM kills of a podvm unit (T-515, githome's G-12). Where the memory
//! of a unit holds in its cgroup leaf (`--memory`), the launcher of
//! `podvm run -d` writes `oom_kills` of `memory.events` of the leaf into
//! the run report with the end of the payload (podvm `0.2.0-beta.32`,
//! PodUtils/podvm T-1969). `podvm wait` gives the exit from the record of
//! the container, which the launcher writes before the report: so a read
//! of a unit with a leaf waits a short time for the end in the report.

use std::time::Duration;

use podrunner_schema::UnitId;
use serde_json::Value;

use super::PodvmUnit;

/// The reads of a report with no end, and the pause between two: about
/// 2 s, as the launcher writes the report right after the record.
const TRIES: u32 = 100;
const PAUSE: Duration = Duration::from_millis(20);

impl PodvmUnit {
    /// The OOM kills of `unit`: the count of its run report.
    pub(super) fn oom_count(&self, unit: &UnitId) -> Option<u64> {
        let leaf = self.units.lock().expect("the units").get(unit).is_some_and(|r| r.memory_leaf);
        settle(|| self.run_report(unit), leaf, TRIES, || std::thread::sleep(PAUSE))
    }
}

/// The count of the report that `read` gives. Where `wait`, a report with
/// no end is read again, at most `tries` times, with `pause` before each
/// read: a unit with no leaf has no count to wait for.
pub(super) fn settle(
    mut read: impl FnMut() -> Option<Value>,
    wait: bool,
    tries: u32,
    mut pause: impl FnMut(),
) -> Option<u64> {
    let mut report = read();
    let mut left = if wait { tries } else { 0 };
    while left > 0 && !report.as_ref().is_some_and(ended) {
        pause();
        report = read();
        left -= 1;
    }
    report.as_ref().and_then(count_of)
}

/// Whether the launcher wrote the end of the run: `exit_code` is a number
/// (`run -d` writes `null` at the start).
fn ended(report: &Value) -> bool {
    report.get("exit_code").is_some_and(Value::is_number)
}

/// `oom_kills` of a report as a whole number. No field (a podvm before
/// `0.2.0-beta.32`, a run with no leaf), `null`, a text such as `-`, a
/// negative number, a fraction and a number past `u64` give none.
pub(super) fn count_of(report: &Value) -> Option<u64> {
    report.get("oom_kills")?.as_u64()
}

#[cfg(test)]
#[path = "oom_tests.rs"]
mod tests;
