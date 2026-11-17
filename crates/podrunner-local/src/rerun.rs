//! The state of a local run and its rerun (T-135). A run keeps the result
//! and the outputs of each cell; a rerun runs the failed cells (or the jobs
//! that `--job` names) and each job after them, and gives each other cell
//! its kept result and outputs with no unit.

use std::collections::{BTreeMap, BTreeSet};

use pod_actions_workflow::{Graph, JobResult, Workflow};
use podrunner_schema::JobId;
use serde_json::{Value, json};

use crate::LocalSource;

/// The end of one cell: its result and its outputs.
pub type Ended = (JobResult, Vec<(String, String)>);

/// The end of one cell of this run, with its id.
pub type CellEnd = (JobId, JobResult, Vec<(String, String)>);

fn result_of(s: &str) -> Option<JobResult> {
    Some(match s {
        "success" => JobResult::Success,
        "failure" => JobResult::Failure,
        "cancelled" => JobResult::Cancelled,
        "skipped" => JobResult::Skipped,
        _ => return None,
    })
}

impl LocalSource {
    /// The end of each cell that ended in this run, as the executor gave it.
    pub fn ends(&self) -> &[CellEnd] {
        &self.cell_ends
    }

    /// The end of a cell that never ran (a stop cancelled it, T-286).
    pub fn note_end(&mut self, job: &JobId, result: JobResult) {
        if !self.cell_ends.iter().any(|(j, _, _)| j == job) {
            self.cell_ends.push((job.clone(), result, Vec::new()));
        }
    }

    /// The kept end of each cell that a rerun does not run.
    pub fn set_kept(&mut self, kept: BTreeMap<String, Ended>) {
        self.kept = kept;
    }

    /// The kept end of a cell, if the rerun keeps it.
    pub fn kept_of(&self, job: &JobId) -> Option<Ended> {
        self.kept.get(job.as_str()).cloned()
    }
}

/// The ends to write after a rerun: a cell that the filter of the rerun
/// left out (`left_out`) keeps its end of the state `before`, and each cell
/// of `before` that this run did not reach stays (T-295).
pub fn with_kept(
    ends: &[CellEnd],
    before: &BTreeMap<String, Ended>,
    left_out: impl Fn(&JobId) -> bool,
) -> Vec<CellEnd> {
    let mut out: Vec<CellEnd> = ends
        .iter()
        .map(|(j, r, o)| match (left_out(j), before.get(j.as_str())) {
            (true, Some((br, bo))) => (j.clone(), *br, bo.clone()),
            _ => (j.clone(), *r, o.clone()),
        })
        .collect();
    for (cell, (r, o)) in before {
        if !out.iter().any(|(j, _, _)| j.as_str() == cell)
            && let Ok(j) = JobId::new(cell)
        {
            out.push((j, *r, o.clone()));
        }
    }
    out
}

/// Write `text` as `DIR/state.json`: the outputs of the jobs can hold a
/// secret, so DIR is 0700 and the file 0600 (#219); the outputs stay as
/// they are, as `--rerun` gives them to the jobs after.
pub fn write_state(dir: &std::path::Path, text: &str) -> std::io::Result<()> {
    write_file(dir, "state.json", text)
}

/// Write `name` in the state directory `dir` (0700), at mode 0600.
pub fn write_file(dir: &std::path::Path, name: &str, text: &str) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    podrunner_broker_fake::runner::private_file(&dir.join(name), text.as_bytes())
}

/// The state file of a run: the digest of the workflow and each end.
pub fn state_json(digest: &str, ends: &[CellEnd]) -> String {
    let cells: Vec<Value> = ends
        .iter()
        .map(|(j, r, o)| {
            let outputs: serde_json::Map<String, Value> = o.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
            json!({ "cell": j.as_str(), "result": r.as_str(), "outputs": outputs })
        })
        .collect();
    serde_json::to_string_pretty(&json!({ "schema": 1, "workflow": digest, "cells": cells })).expect("JSON") + "\n"
}

/// The digest of the workflow and each end of a state file.
pub fn parse_state(text: &str) -> Result<(String, BTreeMap<String, Ended>), String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("the state: {e}"))?;
    let digest = v["workflow"].as_str().ok_or("the state has no workflow digest")?.to_string();
    let mut out = BTreeMap::new();
    for c in v["cells"].as_array().ok_or("the state has no cells")? {
        let cell = c["cell"].as_str().ok_or("a cell with no id")?;
        let result =
            c["result"].as_str().and_then(result_of).ok_or_else(|| format!("the cell {cell} has no result"))?;
        let outputs = c["outputs"]
            .as_object()
            .map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
            .unwrap_or_default();
        out.insert(cell.to_string(), (result, outputs));
    }
    Ok((digest, out))
}

/// The id of the job of a cell id `JOB.N`.
fn job_of(cell: &str) -> &str {
    cell.rsplit_once('.').map_or(cell, |x| x.0)
}

/// The jobs of a rerun: the jobs of `jobs`, or each job with a failed or
/// cancelled cell, and each job after one of them.
pub fn rerun_jobs(w: &Workflow, ends: &BTreeMap<String, Ended>, jobs: &[String]) -> Result<BTreeSet<String>, String> {
    let g = Graph::new(w).map_err(|e| e.to_string())?;
    let mut set: BTreeSet<String> = if jobs.is_empty() {
        ends.iter()
            .filter(|(_, (r, _))| matches!(r, JobResult::Failure | JobResult::Cancelled))
            .map(|(c, _)| job_of(c).to_string())
            .collect()
    } else {
        jobs.iter().cloned().collect()
    };
    // A job that the state does not know (a job of a run that was cut
    // short) runs too.
    for j in &w.jobs {
        if !ends.keys().any(|c| job_of(c) == j.id) {
            set.insert(j.id.clone());
        }
    }
    for job in g.order() {
        if g.needs(job).iter().any(|n| set.contains(*n)) {
            set.insert(job.to_string());
        }
    }
    Ok(set)
}

/// The kept ends: each cell whose job the rerun does not run; with
/// `failed` (`--failed`), also each cell that passed of a job that runs
/// again for its own failed cells, when no need of the job runs again
/// (T-279): its matrix cells that passed stay `kept`.
pub fn kept(
    w: &Workflow,
    ends: &BTreeMap<String, Ended>,
    rerun: &BTreeSet<String>,
    failed: bool,
) -> BTreeMap<String, Ended> {
    let needs_rerun = |job: &str| Graph::new(w).is_ok_and(|g| g.needs(job).iter().any(|n| rerun.contains(*n)));
    ends.iter()
        .filter(|(c, (r, _))| {
            let job = job_of(c);
            !rerun.contains(job) || (failed && *r == JobResult::Success && !needs_rerun(job))
        })
        .map(|(c, e)| (c.clone(), e.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rerun_runs_the_failed_jobs_and_the_jobs_after_them() {
        let text = "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n  b:\n    runs-on: x\n    steps: [{run: b}]\n  c:\n    needs: b\n    runs-on: x\n    steps: [{run: c}]\n";
        let w = pod_actions_workflow::parse(text).unwrap().workflow;
        let ends = |pairs: &[(&str, JobResult)]| -> BTreeMap<String, Ended> {
            pairs.iter().map(|(c, r)| (c.to_string(), (*r, vec![("o".to_string(), c.to_string())]))).collect()
        };
        let e = ends(&[("a.0", JobResult::Success), ("b.0", JobResult::Failure), ("c.0", JobResult::Skipped)]);
        let set = rerun_jobs(&w, &e, &[]).unwrap();
        assert_eq!(set.into_iter().collect::<Vec<_>>(), ["b", "c"]);
        let k = kept(&w, &e, &rerun_jobs(&w, &e, &[]).unwrap(), true);
        assert_eq!(k.keys().collect::<Vec<_>>(), ["a.0"]);
        // A matrix job with a cell that passed and one that failed: the
        // passed cell stays (T-279).
        let m = ends(&[("a.0", JobResult::Success), ("b.0", JobResult::Success), ("b.1", JobResult::Failure)]);
        let k = kept(&w, &m, &rerun_jobs(&w, &m, &[]).unwrap(), true);
        assert_eq!(k.keys().collect::<Vec<_>>(), ["a.0", "b.0"]);
        let k = kept(&w, &m, &rerun_jobs(&w, &m, &["b".into()]).unwrap(), false);
        assert_eq!(k.keys().collect::<Vec<_>>(), ["a.0"], "--job b runs each cell of b");
        let only_a = rerun_jobs(&w, &e, &["a".into()]).unwrap();
        assert_eq!(only_a.into_iter().collect::<Vec<_>>(), ["a"], "--job a: a alone, nothing needs it");
        let short = ends(&[("a.0", JobResult::Success)]);
        assert_eq!(rerun_jobs(&w, &short, &[]).unwrap().len(), 2, "b and c have no end: they run");
        let jobs: Vec<CellEnd> = vec![(JobId::new("a.0").unwrap(), JobResult::Success, vec![("o".into(), "v".into())])];
        let (digest, back) = parse_state(&state_json("sha256:00", &jobs)).unwrap();
        assert_eq!(
            (digest.as_str(), back["a.0"].clone()),
            ("sha256:00", (JobResult::Success, vec![("o".into(), "v".into())]))
        );
    }

    #[test]
    fn a_rerun_keeps_the_end_of_a_cell_that_its_filter_leaves_out() {
        let j = |s: &str| JobId::new(s).unwrap();
        let ends: Vec<CellEnd> = vec![(j("m.0"), JobResult::Success, vec![]), (j("m.1"), JobResult::Skipped, vec![])];
        let before: BTreeMap<String, Ended> = [
            ("m.0".to_string(), (JobResult::Success, vec![])),
            ("m.1".to_string(), (JobResult::Failure, vec![])),
            ("n.0".to_string(), (JobResult::Failure, vec![])),
        ]
        .into();
        let out = with_kept(&ends, &before, |c| c.as_str() == "m.1");
        let got: Vec<(&str, JobResult)> = out.iter().map(|(c, r, _)| (c.as_str(), *r)).collect();
        assert_eq!(got, [("m.0", JobResult::Success), ("m.1", JobResult::Failure), ("n.0", JobResult::Failure)]);
    }
}

#[cfg(test)]
mod mode_tests {
    /// #219: the state of a run is private: DIR 0700, `state.json` 0600.
    #[test]
    fn the_state_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("podrunner-state-mode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        super::write_state(&dir, "{}").unwrap();
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("state.json")), 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
