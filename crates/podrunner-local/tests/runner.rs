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

/// The tests of this file share one runner directory: one runs at a time.
static RUNNER: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Run one offer in the runner of `dir`; give its result and outputs.
fn run(dir: &std::path::Path, s: &mut LocalSource, job: &JobId) -> (JobResult, Vec<(String, String)>, String) {
    let _one = RUNNER.lock().unwrap_or_else(|e| e.into_inner());
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

/// A step reads a secret and the token (T-121, T-122); the log masks the
/// secret.
const SECRETS: &str = r#"on: push
jobs:
  s:
    runs-on: x
    steps:
      - env:
          K: ${{ secrets.API_KEY }}
          T: ${{ github.token }}
        run: |
          test "$K" = made-up-secret-value-42
          test "$T" = podrunner-local-made-up-token
          echo "the secret is $K"
      - run: test "${{ secrets.GITHUB_TOKEN }}" = "${{ github.token }}"
      - run: test -z "${{ secrets.NOT_SET }}"
"#;

#[test]
#[ignore]
fn a_step_reads_a_secret_and_the_token_and_the_log_masks_the_secret() {
    let dir = PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let mut s = LocalSource::new("local", SECRETS, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    s.set_secrets(vec![("API_KEY".into(), "made-up-secret-value-42".into())], None);
    let job = offers(&mut s);
    let (result, _, logs) = run(&dir, &mut s, &job[0]);
    assert_eq!(result, JobResult::Success, "{logs}");
    assert!(logs.contains("the secret is ***"), "{logs}");
    assert!(!logs.contains("made-up-secret-value-42"), "the log has the secret: {logs}");
}

/// T-152: an `env:` of one expression at the levels of the workflow, the
/// job and the step: the runner evaluates it to a mapping.
const ENV_EXPR: &str = r#"on: push
env: ${{ fromJSON('{"W":"wf"}') }}
jobs:
  e:
    runs-on: x
    env: ${{ fromJSON('{"J":"job"}') }}
    steps:
      - env: ${{ secrets }}
        run: test "$W-$J-$API_KEY" = "wf-job-made-up-secret-value-42"
"#;

#[test]
#[ignore]
fn an_env_of_one_expression_gives_a_mapping_at_each_level() {
    let dir = PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let mut s =
        LocalSource::new("local", ENV_EXPR, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    s.set_secrets(vec![("API_KEY".into(), "made-up-secret-value-42".into())], None);
    let job = offers(&mut s);
    let (result, _, logs) = run(&dir, &mut s, &job[0]);
    assert_eq!(result, JobResult::Success, "{logs}");
}
