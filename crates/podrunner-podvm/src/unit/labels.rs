//! The labels and the run report of podvm (T-078, podvm `v0.2.0-beta.5`).
//! Each `run -d` of a unit, and of each of its containers, carries the
//! label `podrunner.unit=ID`: `destroy` removes each container with it,
//! also one whose processes dropped the tag. The `run -d` of the unit
//! writes podvm's report of the run (`--report`) in the unit directory:
//! the digest of the image, the labels, a refusal. For `run -d` the report
//! ends with the start (`detached`): the exit comes from `podvm wait`.

use std::path::Path;
use std::process::{Command, Stdio};

use podrunner_schema::UnitId;
use serde_json::Value;

use super::PodvmUnit;

/// The key of the label of a unit.
pub const LABEL: &str = "podrunner.unit";

/// The file of the report in the directory of a unit.
pub(super) const REPORT: &str = "run-report.json";

/// The arguments of `run` that label a container of `unit`.
pub(super) fn label_args(unit: &str) -> [String; 2] {
    ["--label".into(), format!("{LABEL}={unit}")]
}

impl PodvmUnit {
    /// podvm's report of the `run -d` of `unit`, when podvm wrote it (at
    /// the start, or at a refusal). It holds no value of the environment.
    pub fn run_report(&self, unit: &UnitId) -> Option<Value> {
        let text = std::fs::read(self.unit_dir(unit).join(REPORT)).ok()?;
        serde_json::from_slice(&text).ok()
    }

    /// Remove each container of `store` with the label of `unit`. It gives
    /// the count that `ps` listed.
    pub(super) fn remove_labeled(&self, store: &Path, unit: &str) -> usize {
        let Ok(out) = Command::new(&self.podvm.binary)
            .args(["ps", "-a", "-q", "--filter", &format!("label={LABEL}={unit}")])
            .env("PODVM_STORE", store)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            return 0;
        };
        let ids: Vec<String> = String::from_utf8_lossy(&out.stdout).split_whitespace().map(String::from).collect();
        for id in &ids {
            let _ = Command::new(&self.podvm.binary)
                .args(["rm", "-f", id])
                .env("PODVM_STORE", store)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        ids.len()
    }
}
