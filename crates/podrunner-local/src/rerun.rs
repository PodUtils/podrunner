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

    /// The kept end of each cell that a rerun does not run.
    pub fn set_kept(&mut self, kept: BTreeMap<String, Ended>) {
        self.kept = kept;
    }

    /// The kept end of a cell, if the rerun keeps it.
    pub fn kept_of(&self, job: &JobId) -> Option<Ended> {
        self.kept.get(job.as_str()).cloned()
    }
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

/// The kept ends: each cell whose job the rerun does not run.
pub fn kept(ends: &BTreeMap<String, Ended>, rerun: &BTreeSet<String>) -> BTreeMap<String, Ended> {
    ends.iter().filter(|(c, _)| !rerun.contains(job_of(c))).map(|(c, e)| (c.clone(), e.clone())).collect()
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
        let k = kept(&e, &rerun_jobs(&w, &e, &[]).unwrap());
        assert_eq!(k.keys().collect::<Vec<_>>(), ["a.0"]);
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
}
