//! The corpus of real jobs (T-038): the `ubuntu-24.04` jobs of the podcrates
//! workflows that the fake broker can run. `scripts/corpus.py` writes
//! `corpus/podcrates.json` from a podcrates checkout; the file names the
//! commit and the excluded jobs with the reason.

use serde_json::Value;

use crate::message::{Job, Step};

/// The corpus of the podcrates workflows.
pub const PODCRATES: &str = include_str!("../corpus/podcrates.json");

/// The environment variable of the runner that names a local clone of the
/// source repository. The checkout step clones it.
pub const SOURCE_VAR: &str = "PODRUNNER_CORPUS_SRC";

/// A parsed corpus.
#[derive(Debug, Clone)]
pub struct Corpus {
    /// The commit of the source that the jobs check out.
    pub commit: String,
    doc: Value,
}

fn pairs(v: &Value) -> Vec<(String, String)> {
    let items = v.as_array().map(Vec::as_slice).unwrap_or_default();
    items.iter().filter_map(|p| Some((p[0].as_str()?.to_string(), p[1].as_str()?.to_string()))).collect()
}

/// The step that stands in for `actions/checkout`: a clone of the local
/// source at the commit of the corpus, with all its history (so also the
/// `fetch-depth` of the job). The fake serves no action.
fn checkout(commit: &str) -> Step {
    let script = format!("set -eu\ngit clone -q --no-local \"${SOURCE_VAR}\" .\ngit checkout -q --detach {commit}");
    Step { name: "Checkout".into(), script, condition: String::new(), env: Vec::new() }
}

impl Corpus {
    pub fn parse(text: &str) -> Result<Corpus, String> {
        let doc: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let commit = doc["commit"].as_str().ok_or("the corpus names no commit")?.to_string();
        if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("the commit {commit:?} is not a full id"));
        }
        Ok(Corpus { commit, doc })
    }

    /// The ids of the jobs (`workflow.yml/job`).
    pub fn ids(&self) -> Vec<String> {
        let jobs = self.doc["jobs"].as_array().map(Vec::as_slice).unwrap_or_default();
        jobs.iter().filter_map(|j| j["id"].as_str().map(str::to_string)).collect()
    }

    /// The ids of the excluded jobs, with the reason.
    pub fn excluded(&self) -> Vec<(String, String)> {
        let items = self.doc["excluded"].as_array().map(Vec::as_slice).unwrap_or_default();
        items.iter().filter_map(|e| Some((e["id"].as_str()?.to_string(), e["reason"].as_str()?.to_string()))).collect()
    }

    /// Job `id`, ready for the broker.
    pub fn job(&self, id: &str) -> Option<Job> {
        let jobs = self.doc["jobs"].as_array()?;
        let j = jobs.iter().find(|j| j["id"] == id)?;
        let mut steps = Vec::new();
        for s in j["steps"].as_array()? {
            if s.get("checkout").is_some() {
                steps.push(checkout(&self.commit));
                continue;
            }
            steps.push(Step {
                name: s["name"].as_str()?.to_string(),
                script: s["run"].as_str()?.to_string(),
                condition: s["if"].as_str().unwrap_or_default().to_string(),
                env: pairs(&s["env"]),
            });
        }
        let name = id.rsplit('/').next().unwrap_or(id).to_string();
        Some(Job { name, env: pairs(&j["env"]), steps, container: None, registry: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_corpus_of_podcrates_parses() {
        let c = Corpus::parse(PODCRATES).unwrap();
        let ids = c.ids();
        assert!(ids.contains(&"ci.yml/test".to_string()), "{ids:?}");
        for id in &ids {
            let job = c.job(id).unwrap();
            // Each job checks out first; the fake has no other action.
            assert_eq!(job.steps[0].name, "Checkout", "{id}");
            assert!(job.steps[0].script.contains(&c.commit));
            for s in &job.steps {
                assert!(!s.script.contains("${{"), "{id}: an expression");
                assert!(!s.script.split_whitespace().any(|w| w == "sudo"), "{id}: sudo");
            }
        }
        // Each excluded job has a reason; no job is both.
        for (id, why) in c.excluded() {
            assert!(!why.is_empty() && !ids.contains(&id), "{id}");
        }
        assert!(c.job("no.yml/job").is_none());
    }

    #[test]
    fn a_short_commit_is_refused() {
        assert!(Corpus::parse(r#"{"commit":"abc","jobs":[]}"#).is_err());
    }
}
