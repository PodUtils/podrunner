//! The official runner runs jobs of the corpus (T-038) against the fake
//! broker. Ignored by default: it needs the runner of `tests/runner.rs`, a
//! clone of the source of the corpus at its commit, and what the jobs
//! need (a toolchain, the network).
//!
//!   PODRUNNER_RUNNER_DIR=DIR PODRUNNER_CORPUS_SRC=CLONE \
//!   PODRUNNER_CORPUS_JOBS=upstream.yml/mirrors,ci.yml/record \
//!   cargo test -p podrunner-broker-fake --test corpus -- --ignored

mod common;

use common::run_once;
use podrunner_broker_fake::Broker;
use podrunner_broker_fake::corpus::{Corpus, PODCRATES, SOURCE_VAR};

#[test]
#[ignore]
fn the_jobs_of_the_corpus_succeed() {
    let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k}"));
    let dir = std::path::PathBuf::from(var("PODRUNNER_RUNNER_DIR"));
    let corpus = Corpus::parse(PODCRATES).unwrap();
    let ids: Vec<String> = match var("PODRUNNER_CORPUS_JOBS").as_str() {
        "all" => corpus.ids(),
        list => list.split(',').map(str::to_string).collect(),
    };
    // The jobs need the tools of the caller (rustup, cargo).
    let path = std::env::var("PATH").unwrap_or_default();
    let env = [(SOURCE_VAR, var(SOURCE_VAR)), ("PATH", path)];
    let mut failed = Vec::new();
    for id in &ids {
        let job = corpus.job(id).unwrap_or_else(|| panic!("{id} is not in the corpus"));
        let broker = Broker::start().unwrap();
        broker.queue(job);
        let _ = std::fs::remove_dir_all(dir.join("_work"));
        run_once(&dir, &broker, &env);
        let done = broker.completions();
        let conclusion = done.first().map(|c| c.conclusion.clone()).unwrap_or_default();
        // The log of the job, for the record of the run.
        println!("{}\n{id}: {conclusion}", broker.logs());
        if conclusion != "succeeded" {
            failed.push(id.clone());
        }
    }
    assert!(failed.is_empty(), "failed: {failed:?}");
}
