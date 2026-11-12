//! The official runner runs the jobs of a workflow from the local source,
//! offline, against the fake broker (T-088). Ignored by default: it needs
//! an unpacked release of `actions/runner` in a directory it may change.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-local --test runner -- --ignored

use std::path::PathBuf;
use std::time::Duration;

use pod_actions_workflow::{Base, JobResult};
use podrunner_broker_fake::Broker;
use podrunner_broker_fake::runner::run_once;
use podrunner_local::LocalSource;
use podrunner_schema::JobId;
use podrunner_source::{Event, JobSource};

// Each marker is computed by the shell: the echo of a script cannot match
// it. `fails` must be skipped: its step of `failure()` writes a file.
const FLOW: &str = r#"on: push
env:
  W: wf
jobs:
  build:
    runs-on: x
    strategy:
      matrix:
        os: [linux]
    env:
      OS: ${{ matrix.os }}
    outputs:
      out: ${{ steps.s.outputs.v }}
    steps:
      - id: s
        run: echo "v=${{ matrix.os }}-$OS-$W-$((6*7))" >> "$GITHUB_OUTPUT"
      - if: steps.s.outputs.v == 'linux-linux-wf-42'
        run: echo "podrunner-if-$((7*7))"
      - env:
          S: ${{ steps.s.outputs.v }}
        run: echo "podrunner-env-$S" | tr a-z A-Z
      - if: failure()
        run: touch failure-ran
  after:
    needs: build
    runs-on: x
    steps:
      - run: echo "podrunner-needs-${{ needs.build.outputs.out }}-${{ needs.build.result }}" | rev
"#;

fn offers(s: &mut LocalSource) -> Vec<JobId> {
    s.next(10, Duration::ZERO)
        .unwrap()
        .into_iter()
        .filter_map(|e| match e {
            Event::Offer { offer } => Some(offer.job_id),
            _ => None,
        })
        .collect()
}

/// Run one offer in the runner of `dir`; give its result and outputs.
fn run(dir: &std::path::Path, s: &mut LocalSource, job: &JobId) -> (JobResult, Vec<(String, String)>, String) {
    let broker = Broker::start().unwrap();
    broker.queue(s.broker_job(job).unwrap());
    let _ = std::fs::remove_dir_all(dir.join("_work"));
    run_once(dir, &broker, &[]);
    let done = broker.completions();
    assert_eq!(done.len(), 1, "{:?}", broker.seen());
    let result = if done[0].conclusion == "succeeded" { JobResult::Success } else { JobResult::Failure };
    (result, done[0].outputs.clone(), broker.logs())
}

#[test]
#[ignore]
fn the_official_runner_runs_a_workflow_of_the_local_source() {
    let dir = PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let mut s = LocalSource::new("local", FLOW, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    let first = offers(&mut s);
    assert_eq!(first.len(), 1);
    let (result, outputs, logs) = run(&dir, &mut s, &first[0]);
    assert_eq!(result, JobResult::Success, "{logs}");
    // The matrix, the env of the job and of the workflow, a step output.
    assert_eq!(outputs, [("out".to_string(), "linux-linux-wf-42".to_string())], "{logs}");
    // A step `if:` on a step output; a step env from a step output.
    assert!(logs.contains("podrunner-if-49"), "{logs}");
    assert!(logs.contains("PODRUNNER-ENV-LINUX-LINUX-WF-42"), "{logs}");
    assert!(!dir.join("_work/app/app/failure-ran").exists(), "a step of failure() ran");
    s.complete(&first[0], result, outputs).unwrap();

    let next = offers(&mut s);
    assert_eq!(next.len(), 1);
    let (result, _, logs) = run(&dir, &mut s, &next[0]);
    assert_eq!(result, JobResult::Success, "{logs}");
    // `needs.build.outputs.out` and `needs.build.result`, reversed by the
    // shell.
    assert!(logs.contains(&"podrunner-needs-linux-linux-wf-42-success".chars().rev().collect::<String>()), "{logs}");
    s.complete(&next[0], result, vec![]).unwrap();
    assert!(s.is_finished());
}
