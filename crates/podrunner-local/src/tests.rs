use pod_actions_workflow::{Base, JobResult};
use podrunner_schema::{Credential, CredentialFile, JobId, Secret};
use podrunner_source::conformance::{self, Fixture};
use podrunner_source::{Event, JobSource};
use std::time::Duration;

use crate::{LocalSource, Minter};

fn minter() -> Minter {
    Box::new(|job: &JobId, runner: &str| Credential {
        files: vec![CredentialFile { name: ".runner".into(), content: Secret::new(format!("{runner} {job}")) }],
    })
}

fn source(text: &str) -> LocalSource {
    LocalSource::new("local-1", text, Base::default(), "x86_64", minter()).unwrap()
}

/// The ids of the offers of one `next`.
fn offers(s: &mut LocalSource) -> Vec<String> {
    let events = s.next(100, Duration::ZERO).unwrap();
    events
        .iter()
        .map(|e| match e {
            Event::Offer { offer } => offer.job_id.to_string(),
            other => panic!("not an offer: {other:?}"),
        })
        .collect()
}

fn id(s: &str) -> JobId {
    JobId::new(s).unwrap()
}

const TWO: &str = "on: push\njobs:\n  a:\n    runs-on: ubuntu-24.04\n    steps:\n      - run: echo a\n  \
                   b:\n    runs-on: [self-hosted, x64]\n    steps:\n      - run: echo b\n";

struct Two;

impl Fixture for Two {
    fn fresh(&self) -> Box<dyn JobSource> {
        Box::new(source(TWO))
    }
}

#[test]
fn the_local_source_passes_the_source_conformance_suite() {
    let reports = conformance::run(&Two, &[]);
    let failed: Vec<String> = reports.iter().filter(|r| r.failure.is_some()).map(ToString::to_string).collect();
    assert!(failed.is_empty(), "{failed:#?}");
    assert_eq!(reports.len(), 12);
}

#[test]
fn an_offer_has_the_labels_of_runs_on() {
    let mut s = source(TWO);
    let events = s.next(1, Duration::ZERO).unwrap();
    let Event::Offer { offer } = &events[0] else { panic!("{events:?}") };
    assert_eq!(offer.job_id.to_string(), "a.0");
    assert_eq!(offer.fields["labels"], serde_json::json!(["ubuntu-24.04"]));
    assert_eq!(offer.fields["arch"], "x86_64");
    assert_eq!(offers(&mut s), ["b.0"]);
    assert_eq!(s.cell(&id("b.0")).unwrap().instance.runs_on.as_ref().unwrap().labels, ["self-hosted", "x64"]);
}

const CHAIN: &str = "on: push\njobs:\n  build:\n    runs-on: x\n    strategy:\n      matrix:\n        os: [a, b]\n    \
                     steps:\n      - run: echo ${{ matrix.os }}\n  test:\n    needs: build\n    runs-on: x\n    \
                     steps:\n      - run: echo t\n  report:\n    needs: test\n    if: failure()\n    runs-on: x\n    \
                     steps:\n      - run: echo r\n";

#[test]
fn needs_order_the_offers_and_a_matrix_gives_one_offer_for_each_cell() {
    let mut s = source(CHAIN);
    assert_eq!(offers(&mut s), ["build.0", "build.1"]);
    assert_eq!(s.cell(&id("build.1")).unwrap().instance.matrix.get("os").unwrap().as_str(), Some("b"));
    // `test` waits for each cell of `build`.
    s.complete(&id("build.0"), JobResult::Success, vec![]).unwrap();
    assert!(offers(&mut s).is_empty());
    s.complete(&id("build.1"), JobResult::Success, vec![]).unwrap();
    assert_eq!(s.result("build"), Some(JobResult::Success));
    assert_eq!(offers(&mut s), ["test.0"]);
    // `report` runs only on a failure: a success skips it, with its reason.
    s.complete(&id("test.0"), JobResult::Success, vec![]).unwrap();
    assert!(offers(&mut s).is_empty());
    assert!(s.is_finished());
    assert_eq!(s.result("report"), Some(JobResult::Skipped));
    assert_eq!(s.skips().len(), 1);
    assert_eq!(s.skips()[0].job, "report");
    // A status function: no implicit `success() &&`.
    assert_eq!(s.skips()[0].condition, "failure()");
    assert!(s.complete(&id("test.0"), JobResult::Success, vec![]).is_err(), "a second end");
}

#[test]
fn a_failed_cell_fails_its_job_and_runs_the_job_of_failure() {
    let mut s = source(CHAIN);
    offers(&mut s);
    s.complete(&id("build.0"), JobResult::Success, vec![]).unwrap();
    s.complete(&id("build.1"), JobResult::Failure, vec![]).unwrap();
    assert_eq!(s.result("build"), Some(JobResult::Failure));
    // `test` has no status function: a failed need skips it. `report`
    // reads `failure()` of each job before it, so it runs.
    assert_eq!(offers(&mut s), ["report.0"]);
    assert_eq!(s.result("test"), Some(JobResult::Skipped));
}

#[test]
fn the_outputs_of_a_job_reach_the_needs_of_the_next_one() {
    let text = "on: push\njobs:\n  a:\n    runs-on: x\n    outputs:\n      os: ${{ steps.s.outputs.os }}\n    \
                steps:\n      - id: s\n        run: echo\n  b:\n    needs: a\n    runs-on: ${{ needs.a.outputs.os }}\n    \
                steps:\n      - run: echo b\n";
    let mut s = source(text);
    assert_eq!(offers(&mut s), ["a.0"]);
    s.complete(&id("a.0"), JobResult::Success, vec![("os".into(), "linux-arm".into())]).unwrap();
    assert_eq!(offers(&mut s), ["b.0"]);
    assert_eq!(s.cell(&id("b.0")).unwrap().instance.runs_on.as_ref().unwrap().labels, ["linux-arm"]);
}

#[test]
fn a_bad_file_and_a_cycle_are_refused_by_name() {
    let no_jobs = LocalSource::new("l", "on: push\n", Base::default(), "x86_64", minter());
    assert!(no_jobs.is_err());
    let cycle = "on: push\njobs:\n  a:\n    needs: b\n    runs-on: x\n    steps: [{run: x}]\n  \
                 b:\n    needs: a\n    runs-on: x\n    steps: [{run: x}]\n";
    let e = LocalSource::new("l", cycle, Base::default(), "x86_64", minter()).err().unwrap();
    assert!(e.contains("cycle"), "{e}");
}

#[test]
fn mint_gives_the_credential_of_the_minter() {
    let mut s = source(TWO);
    offers(&mut s);
    s.claim(&id("a.0"), "r-1").unwrap();
    let c = s.mint(&id("a.0"), "r-1").unwrap();
    assert_eq!(c.files[0].content.expose(), "r-1 a.0");
}
