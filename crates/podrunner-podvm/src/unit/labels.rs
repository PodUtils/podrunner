//! The labels and the run report of podvm (T-078, podvm `v0.2.0-beta.5`).
//! Each `run -d` of a unit, and of each of its containers, carries the
//! label `podrunner.unit=ID`: `destroy` removes each container with it,
//! also one whose processes dropped the tag. The `run -d` of the unit
//! writes podvm's report of the run (`--report`) in the unit directory:
//! the digest of the image, the labels, a refusal. For `run -d` the report
//! ends with the start (`detached`): the exit comes from `podvm wait`.

use std::path::Path;
use std::process::Stdio;

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

/// The arguments of `run -d` of `unit`: its label and `fixup_args`.
pub(super) fn run_args(podvm: &crate::Podvm, unit: &str) -> Vec<String> {
    label_args(unit).into_iter().chain(fixup_args(podvm).iter().map(|a| a.to_string())).collect()
}

/// The first podvm that is known to take `--no-source-fixup`.
const NO_SOURCE_FIXUP: &str = "0.2.0-beta.30";

/// `--no-source-fixup` for `run` and `exec` of `podvm`: with the fixup,
/// podvm asks each host of the package sources of the image before the
/// payload, also under the egress hold of a unit (T-332,
/// PodUtils/podvm#464). The answer of each binary is kept.
pub(super) fn fixup_args(podvm: &crate::Podvm) -> &'static [&'static str] {
    if at_least(podvm, NO_SOURCE_FIXUP) { &["--no-source-fixup"] } else { &[] }
}

/// Whether `podvm` is `version` or later. The answer of each binary for
/// each version is kept.
pub(super) fn at_least(podvm: &crate::Podvm, version: &str) -> bool {
    type Seen = std::collections::BTreeMap<(std::path::PathBuf, String), bool>;
    static SEEN: std::sync::Mutex<Seen> = std::sync::Mutex::new(std::collections::BTreeMap::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    *seen.entry((podvm.binary.clone(), version.to_string())).or_insert_with(|| {
        let low = crate::probe::Version::parse(version).expect("a version");
        podvm.version().is_some_and(|v| v >= low)
    })
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
        let Ok(out) = self
            .podvm
            .command()
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
            let _ = self
                .podvm
                .command()
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

/// `argv` with no `--hostname NAME`.
pub(super) fn no_hostname<'a>(argv: &[&'a str], name: &str) -> Vec<&'a str> {
    let mut out = Vec::with_capacity(argv.len());
    let mut it = argv.iter().peekable();
    while let Some(a) = it.next() {
        if *a == "--hostname" && it.peek().is_some_and(|n| **n == name) {
            it.next();
            continue;
        }
        out.push(*a);
    }
    out
}

/// The names of the processes `pids` but `waiter`, each read once from
/// `/proc`: a process that ended after the list has no name, and is gone,
/// not an orphan (T-159).
pub(super) fn live_names(pids: &[u32], waiter: Option<u32>) -> Vec<String> {
    pids.iter()
        .filter(|p| Some(**p) != waiter)
        .filter_map(|p| std::fs::read_to_string(format!("/proc/{p}/comm")).ok())
        .map(|c| c.trim().to_string())
        .collect()
}

/// With no live supervisor of podvm, but processes of the unit, the orphans
/// went to init: no count is true then (design test 15). The `podnet pod` of
/// the hold carries the tag and can outlive the payload: it is no process of
/// the payload.
pub(super) fn orphaned(live: &[String]) -> bool {
    !live.iter().any(|c| c.starts_with("podvm")) && live.iter().any(|c| !c.starts_with("podnet"))
}

#[cfg(test)]
mod tests {
    /// T-159: a process that ended after the list is no orphan.
    #[test]
    fn a_process_that_ended_is_no_orphan() {
        let gone = u32::MAX - 7;
        assert!(super::live_names(&[gone], None).is_empty());
        assert!(!super::orphaned(&super::live_names(&[gone], None)));
        let me = std::process::id();
        assert_eq!(super::live_names(&[me, gone], None).len(), 1);
        assert!(super::live_names(&[me], Some(me)).is_empty());
        assert!(super::orphaned(&["sh".into()]));
        assert!(!super::orphaned(&["podvm".into(), "sh".into()]));
        assert!(!super::orphaned(&["podnet".into()]));
    }

    /// T-182: the start with no name drops `--hostname NAME` only.
    #[test]
    fn the_start_with_no_name_drops_the_name_only() {
        let argv = ["run", "-d", "--hostname", "podrunner-u-1", "-e", "A=--hostname", "img"];
        assert_eq!(super::no_hostname(&argv, "podrunner-u-1"), ["run", "-d", "-e", "A=--hostname", "img"]);
    }
}
