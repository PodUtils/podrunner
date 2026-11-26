//! The official runner runs canned jobs against the fake broker, offline.
//! Ignored by default: it needs an unpacked release of `actions/runner`
//! (v2.338.2, `images/runner/runner.toml`) in a directory it may change.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-broker-fake --test runner -- --ignored

use std::sync::Mutex;
use std::time::Duration;

use podrunner_broker_fake::{Broker, Job};

mod common;

use common::{front_stand_in, front_stand_in_unix, run_once};

/// The tests share one runner directory and its configuration: one at a
/// time.
static DIR: Mutex<()> = Mutex::new(());

#[test]
#[ignore]
fn the_official_runner_runs_a_canned_job_offline() {
    let _one = DIR.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::path::PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let broker = Broker::start().unwrap();
    // A computed marker: the echo of the script cannot match it.
    let ok = Job::scripts("hello", &["echo podrunner-step-$((6*7)) | tee marker.txt"]);
    broker.queue(ok);
    let _ = std::fs::remove_dir_all(dir.join("_work"));
    assert!(run_once(&dir, &broker, &[]).success());
    let done = broker.completions();
    assert_eq!(done.len(), 1, "{:?}", broker.seen());
    assert_eq!(done[0].conclusion, "succeeded");
    let marker = std::fs::read_to_string(dir.join("_work/app/app/marker.txt")).unwrap();
    assert_eq!(marker.trim(), "podrunner-step-42");
    // The output of the step reaches the results service as a log.
    assert!(broker.logs().contains("podrunner-step-42"), "{:?}", broker.seen());
    assert!(!broker.step_updates().is_empty());

    // A step that fails fails the job, and a later step does not run.
    let broker = Broker::start().unwrap();
    broker.queue(Job::scripts("fails", &["exit 3", "echo no > second.txt"]));
    let _ = std::fs::remove_dir_all(dir.join("_work"));
    run_once(&dir, &broker, &[]);
    std::thread::sleep(Duration::from_millis(100));
    let done = broker.completions();
    assert_eq!(done[0].conclusion, "failed", "{done:?}");
    assert!(!dir.join("_work/app/app/second.txt").exists());
}

/// An action with expression defaults, in the form of `actions/setup-python`
/// (T-541). The composite step reads the inputs through expressions too.
const DEFAULTS: &str = r#"name: defaults
inputs:
  token:
    default: ${{ github.server_url == 'https://github.com' && github.token || '' }}
  total:
    default: ${{ strategy.job-total }}
  matrix:
    default: ${{ toJSON(matrix) }}
runs:
  using: composite
  steps:
    - shell: bash
      env:
        T: ${{ inputs.token }}
        N: ${{ inputs.total }}
        M: ${{ inputs.matrix }}
      run: |
        echo "defaults-token-length=${#T}"
        echo "defaults-total=$N"
        echo "defaults-matrix=$M"
        test "${#T}" -gt 0 && test "$N" = 1 && test "$M" = null
        echo "defaults-ok-$((6*7))"
"#;

/// T-541: a step `uses:` of an action whose inputs have expression
/// defaults. The runner evaluates each default with the context data of
/// the job message, and the canned job of the fake broker has each context
/// that the schema of the default names (`strategy`, `matrix`).
#[test]
#[ignore]
fn an_action_input_takes_its_expression_default() {
    let _one = DIR.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::path::PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    const SHA: &str = "5410000000000000000000000000000000000541";
    let actions = std::env::temp_dir().join(format!("podrunner-t541-{}", std::process::id()));
    let _gone = Gone(actions.clone());
    let top = actions.join("work/example-org-defaults-5410000");
    std::fs::create_dir_all(&top).unwrap();
    std::fs::write(top.join("action.yml"), DEFAULTS).unwrap();
    let out = actions.join(podrunner_broker_fake::actions::archive_name("example-org/defaults", SHA));
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    let tar = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&out)
        .arg("-C")
        .arg(actions.join("work"))
        .arg(top.file_name().unwrap())
        .status()
        .unwrap();
    assert!(tar.success());
    let broker = Broker::start().unwrap();
    broker.serve_actions(Some(&actions), Default::default());
    let mut job = Job::scripts("defaults", &[]);
    let reference = serde_json::json!({
        "type": "repository", "repositoryType": "GitHub", "name": "example-org/defaults", "ref": SHA
    });
    job.overrides.insert(
        "steps".into(),
        serde_json::json!([{
            "type": "action", "reference": reference, "id": "00000000-0000-0000-0000-000000000100",
            "name": "defaults", "contextName": "defaults", "displayName": "defaults", "condition": "success()",
            "inputs": { "type": 2, "map": [] },
        }]),
    );
    broker.queue(job);
    let _ = std::fs::remove_dir_all(dir.join("_work"));
    run_once(&dir, &broker, &[]);
    std::thread::sleep(Duration::from_millis(100));
    let done = broker.completions();
    let logs = broker.logs();
    assert!(!logs.contains("The template is not valid"), "{logs}");
    assert_eq!(done.len(), 1, "{:?}", broker.seen());
    assert_eq!(done[0].conclusion, "succeeded", "{done:?}\n{logs}");
    assert!(logs.contains("defaults-total=1") && logs.contains("defaults-matrix=null"), "{logs}");
    assert!(logs.contains("defaults-ok-42"), "{logs}");
}

/// Removes its directory at the drop, also on a panic.
struct Gone(std::path::PathBuf);

impl Drop for Gone {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// T-066: a "version too old" answer of the broker gives exit 7 with the
/// variable of the start environment of slots, and another code with no
/// variable (the plant).
#[test]
#[ignore]
fn version_too_old_gives_exit_7_with_the_variable() {
    let _one = DIR.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::path::PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let broker = Broker::start().unwrap();
    broker.refuse_the_version();
    let with = run_once(&dir, &broker, &[("ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE", "1".into())]);
    assert_eq!(with.code(), Some(7), "{:?}", broker.seen());
    let without = run_once(&dir, &broker, &[]);
    assert_ne!(without.code(), Some(7));
}

/// T-175: the job-started hook of the runner image
/// (`images/runner/podrunner-job-started.sh`, in `hooks/` of the runner
/// root, as in the image) ends a job that is not the job of the unit
/// before its first step. The fake broker gives the repository
/// `example-org/app` and the run id of the request (1).
#[test]
#[ignore]
fn a_job_started_hook_ends_a_foreign_job_before_its_first_step() {
    let _one = DIR.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::path::PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    // In the binary: runner-musl.yml runs this test where the tree is not.
    let script = include_str!("../../../images/runner/podrunner-job-started.sh");
    std::fs::create_dir_all(dir.join("hooks")).unwrap();
    let hook = dir.join("hooks/podrunner-job-started.sh");
    std::fs::write(&hook, script).unwrap();
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let job = dir.join(".podrunner-job");
    // The own job (another case), another repository, another run, and no
    // file (the local mode).
    for (want, ok) in [
        (Some("Example-Org/App 1\n"), true),
        (Some("example-org/other 1\n"), false),
        (Some("example-org/app 2\n"), false),
        (None, true),
    ] {
        match want {
            Some(w) => std::fs::write(&job, w).unwrap(),
            None => {
                let _ = std::fs::remove_file(&job);
            }
        }
        let broker = Broker::start().unwrap();
        // T-682: a step of `if: always()` runs after a failed step. On a
        // refused job it must not run: the hook ends the worker.
        let mut own = Job::scripts("own", &["echo step > marker.txt", "echo always > always.txt"]);
        own.steps[1].condition = "always()".into();
        broker.queue(own);
        let _ = std::fs::remove_dir_all(dir.join("_work"));
        let err = dir.join("podrunner-stderr.txt");
        let env = [("ACTIONS_RUNNER_HOOK_JOB_STARTED", hook.display().to_string())];
        let status = podrunner_broker_fake::runner::run_once_into(&dir, &broker, &env, &err);
        std::thread::sleep(Duration::from_millis(100));
        let done = broker.completions();
        assert_eq!(done[0].conclusion, if ok { "succeeded" } else { "failed" }, "{want:?}: {done:?}");
        // T-552: the runner exits 0 either way; an ended job leaves the
        // line of the hook on the stderr of the runner (the log of a unit).
        assert!(status.success(), "{want:?}: {status:?}");
        let log = std::fs::read_to_string(&err).unwrap_or_default();
        assert_eq!(log.contains("podrunner-job-started: ended: "), !ok, "{want:?}: {log}");
        assert_eq!(dir.join("_work/app/app/marker.txt").exists(), ok, "{want:?}: the step ran or not");
        assert_eq!(dir.join("_work/app/app/always.txt").exists(), ok, "{want:?}: the always() step ran or not");
        // T-201: a kept job has no annotation of a foreign job. A refused
        // job loses its annotation with its worker (T-682); the host reports
        // the code of the line above (T-552).
        if ok {
            assert!(!broker.logs().contains("##[error]podrunner: this runner was placed"), "{}", broker.logs());
        }
    }
    let _ = std::fs::remove_file(&job);
}

/// T-493: for a job of another run, the hook asks serve through the proxy
/// of the unit; `keep` runs the job, `end` ends it with the code in its
/// annotation.
#[test]
#[ignore]
fn the_job_started_hook_asks_serve_for_a_job_of_another_run() {
    let _one = DIR.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::path::PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let script = include_str!("../../../images/runner/podrunner-job-started.sh");
    std::fs::create_dir_all(dir.join("hooks")).unwrap();
    let hook = dir.join("hooks/podrunner-job-started.sh");
    std::fs::write(&hook, script).unwrap();
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let job = dir.join(".podrunner-job");
    std::fs::write(&job, "example-org/app 2\n").unwrap();
    for (verdict, ok) in [(r#"{"verdict":"keep"}"#, true), (r#"{"verdict":"end","code":"tier-mismatch"}"#, false)] {
        let (port, seen) = front_stand_in(verdict);
        let broker = Broker::start().unwrap();
        broker.queue(Job::scripts("own", &["echo step > marker.txt"]));
        let _ = std::fs::remove_dir_all(dir.join("_work"));
        let env = [
            ("ACTIONS_RUNNER_HOOK_JOB_STARTED", hook.display().to_string()),
            ("HTTP_PROXY", format!("http://127.0.0.1:{port}")),
            ("NO_PROXY", "127.0.0.1,localhost".to_string()),
            ("no_proxy", "127.0.0.1,localhost".to_string()),
        ];
        let err = dir.join("podrunner-stderr.txt");
        podrunner_broker_fake::runner::run_once_into(&dir, &broker, &env, &err);
        std::thread::sleep(Duration::from_millis(100));
        let done = broker.completions();
        assert_eq!(done[0].conclusion, if ok { "succeeded" } else { "failed" }, "{verdict}: {done:?}");
        assert_eq!(dir.join("_work/app/app/marker.txt").exists(), ok, "{verdict}: the step ran or not");
        let asked = seen.lock().unwrap().clone();
        assert_eq!(asked.len(), 1, "one question: {asked:?}");
        assert!(asked[0].starts_with("POST http://control.podrunner.test/v1/assigned "), "{asked:?}");
        assert!(
            asked[0].contains(r#""run_id":1"#) && asked[0].contains(r#""repository":"example-org/app""#),
            "{asked:?}"
        );
        // T-653: the ref and the repository id of the job, for its cache.
        assert!(asked[0].contains(r#""ref":"refs/heads/main""#) && asked[0].contains(r#""repo_id":4242"#), "{asked:?}");
        // The code reaches the host in the line of the hook (T-552); the
        // job log ends with the worker (T-682).
        let log = std::fs::read_to_string(&err).unwrap_or_default();
        assert_eq!(log.contains("podrunner-job-started: ended: tier-mismatch"), !ok, "{log}");
    }
    let _ = std::fs::remove_file(&job);
}

/// T-660: for a job of the placed run, the hook marks the first step at
/// the front of the unit (`POST /v1/started`, with no fact of the job), and
/// the job runs. A front that does not answer does not end the job.
#[test]
#[ignore]
fn the_job_started_hook_marks_the_first_step_of_its_own_job() {
    let _one = DIR.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::path::PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let script = include_str!("../../../images/runner/podrunner-job-started.sh");
    std::fs::create_dir_all(dir.join("hooks")).unwrap();
    let hook = dir.join("hooks/podrunner-job-started.sh");
    std::fs::write(&hook, script).unwrap();
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let job = dir.join(".podrunner-job");
    std::fs::write(&job, "Example-Org/App 1\n").unwrap();
    for (strict, answers) in [(false, true), (false, false), (true, true), (true, false)] {
        let (port, seen) = if answers {
            front_stand_in(if strict { r#"{"verdict":"keep"}"# } else { r#"{"ok":true}"# })
        } else {
            // A port with no listener: the connection is refused.
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            (l.local_addr().unwrap().port(), Default::default())
        };
        let broker = Broker::start().unwrap();
        broker.queue(Job::scripts("own", &["echo step > marker.txt"]));
        let _ = std::fs::remove_dir_all(dir.join("_work"));
        let env = [
            ("ACTIONS_RUNNER_HOOK_JOB_STARTED", hook.display().to_string()),
            ("HTTP_PROXY", format!("http://localhost:{port}")),
            ("http_proxy", format!("http://localhost:{port}")),
            ("PODRUNNER_ASSIGNMENT_HOOK", if strict { "1" } else { "0" }.into()),
            ("NO_PROXY", "127.0.0.1".to_string()),
            ("no_proxy", "127.0.0.1".to_string()),
            ("NODE_USE_ENV_PROXY", "1".to_string()),
        ];
        let err = dir.join("podrunner-stderr.txt");
        podrunner_broker_fake::runner::run_once_into(&dir, &broker, &env, &err);
        std::thread::sleep(Duration::from_millis(100));
        let done = broker.completions();
        let succeeds = !strict || answers;
        assert_eq!(done[0].conclusion, if succeeds { "succeeded" } else { "failed" }, "{done:?} {}", broker.logs());
        assert_eq!(dir.join("_work/app/app/marker.txt").exists(), succeeds);
        let asked: Vec<String> = seen.lock().unwrap().clone();
        assert_eq!(asked.len(), usize::from(answers), "one mark: {asked:?}");
        if strict && answers {
            assert!(asked[0].starts_with("POST http://control.podrunner.test/v1/assigned "), "{asked:?}");
        } else if answers {
            assert!(asked[0].starts_with("POST http://control.podrunner.test/v1/started "), "{asked:?}");
            assert!(asked[0].ends_with("\n{}"), "no fact of the job: {asked:?}");
        }
        let log = std::fs::read_to_string(&err).unwrap_or_default();
        assert_eq!(log.contains("podrunner-job-started: ended: no-answer"), !succeeds, "{log}");
    }
    let _ = std::fs::remove_file(&job);
}

/// T-530: a podvm unit has no proxy variable; the hook asks through the
/// egress socket that `PODRUNNER_EGRESS` names, and `keep` runs the job.
#[test]
#[ignore]
fn the_job_started_hook_asks_through_the_egress_socket() {
    let _one = DIR.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::path::PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let script = include_str!("../../../images/runner/podrunner-job-started.sh");
    std::fs::create_dir_all(dir.join("hooks")).unwrap();
    let hook = dir.join("hooks/podrunner-job-started.sh");
    std::fs::write(&hook, script).unwrap();
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let job = dir.join(".podrunner-job");
    std::fs::write(&job, "example-org/app 2\n").unwrap();
    let sock = std::env::temp_dir().join(format!("podrunner-egress-{}.sock", std::process::id()));
    let seen = front_stand_in_unix(r#"{"verdict":"keep"}"#, &sock);
    let broker = Broker::start().unwrap();
    broker.queue(Job::scripts("own", &["echo step > marker.txt"]));
    let _ = std::fs::remove_dir_all(dir.join("_work"));
    let env = [
        ("ACTIONS_RUNNER_HOOK_JOB_STARTED", hook.display().to_string()),
        ("PODRUNNER_EGRESS", format!("http://unix:{}", sock.display())),
    ];
    run_once(&dir, &broker, &env);
    std::thread::sleep(Duration::from_millis(100));
    let done = broker.completions();
    assert_eq!(done[0].conclusion, "succeeded", "{done:?} {}", broker.logs());
    assert!(dir.join("_work/app/app/marker.txt").exists(), "the step ran");
    let asked = seen.lock().unwrap().clone();
    assert_eq!(asked.len(), 1, "one question: {asked:?}");
    assert!(asked[0].starts_with("POST http://control.podrunner.test/v1/assigned "), "{asked:?}");
    let _ = std::fs::remove_file(&job);
    let _ = std::fs::remove_file(&sock);
}

/// The hook sends `head_is_fork` true when the head of the pull request is
/// another repository than its base, by id (else by name, in any case): a
/// pull request from the root of a fork network has a head with `fork`
/// false. It sends none when it cannot tell. The hook runs alone here, with
/// the node of the runner.
#[test]
#[ignore]
fn the_job_started_hook_sends_a_head_that_is_not_the_base_as_a_fork() {
    let _one = DIR.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::path::PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let script = include_str!("../../../images/runner/podrunner-job-started.sh");
    std::fs::create_dir_all(dir.join("hooks")).unwrap();
    let hook = dir.join("hooks/podrunner-job-started.sh");
    std::fs::write(&hook, script).unwrap();
    let job = dir.join(".podrunner-job");
    std::fs::write(&job, "example-org/app 2\n").unwrap();
    let event = dir.join("podrunner-event.json");
    let repo = |id: Option<u64>, name: &str, fork: bool| match id {
        Some(id) => format!(r#"{{"id":{id},"full_name":"{name}","fork":{fork}}}"#),
        None => format!(r#"{{"full_name":"{name}","fork":{fork}}}"#),
    };
    let pr = |head: String, base: Option<String>| match base {
        Some(b) => format!(r#"{{"pull_request":{{"head":{{"repo":{head}}},"base":{{"repo":{b}}}}}}}"#),
        None => format!(r#"{{"pull_request":{{"head":{{"repo":{head}}}}}}}"#),
    };
    for (why, body, want) in [
        (
            "the root of the network into a fork",
            pr(repo(Some(1), "up/app", false), Some(repo(Some(2), "example-org/app", true))),
            Some(true),
        ),
        (
            "a branch of the base, which is a fork",
            pr(repo(Some(2), "example-org/app", true), Some(repo(Some(2), "example-org/app", true))),
            Some(false),
        ),
        (
            "no ids, one name in another case",
            pr(repo(None, "Example-Org/App", false), Some(repo(None, "example-org/app", false))),
            Some(false),
        ),
        ("no ids, other names", pr(repo(None, "x/app", true), Some(repo(None, "example-org/app", false))), Some(true)),
        ("no base", pr(repo(Some(1), "up/app", true), None), None),
        // GitHub gives a null head repository when the fork is deleted.
        ("a deleted fork", pr("null".to_string(), Some(repo(Some(2), "example-org/app", false))), None),
        ("no pull request", "{}".to_string(), None),
    ] {
        std::fs::write(&event, body).unwrap();
        let (port, seen) = front_stand_in(r#"{"verdict":"keep"}"#);
        let out = std::process::Command::new("sh")
            .arg(&hook)
            .env_clear()
            .env("GITHUB_REPOSITORY", "example-org/app")
            .env("GITHUB_RUN_ID", "1")
            .env("GITHUB_EVENT_NAME", "pull_request_target")
            .env("GITHUB_EVENT_PATH", &event)
            .env("HTTP_PROXY", format!("http://127.0.0.1:{port}"))
            .output()
            .unwrap();
        assert!(out.status.success(), "{why}: {out:?}");
        let asked = seen.lock().unwrap().clone();
        assert_eq!(asked.len(), 1, "{why}: {asked:?}");
        let body: serde_json::Value = serde_json::from_str(asked[0].split("\n\n").last().unwrap()).unwrap();
        assert_eq!(body["head_is_fork"].as_bool(), want, "{why}: {body}");
    }
    let _ = std::fs::remove_file(&job);
    let _ = std::fs::remove_file(&event);
}

/// T-653: the hook sends `GITHUB_REF` as `ref` and `GITHUB_REPOSITORY_ID`
/// as `repo_id`, each only when it is set and in the bounds of the wire (a
/// ref of at most 256 bytes with no control character, a whole number).
/// The hook runs alone here, with the node of the runner.
#[test]
#[ignore]
fn the_job_started_hook_sends_the_ref_and_the_repository_id_in_their_bounds() {
    let _one = DIR.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::path::PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let script = include_str!("../../../images/runner/podrunner-job-started.sh");
    std::fs::create_dir_all(dir.join("hooks")).unwrap();
    let hook = dir.join("hooks/podrunner-job-started.sh");
    std::fs::write(&hook, script).unwrap();
    let job = dir.join(".podrunner-job");
    std::fs::write(&job, "example-org/app 2\n").unwrap();
    let long = format!("refs/heads/{}", "b".repeat(245));
    let longer = format!("{long}b");
    for (why, git_ref, id, want_ref, want_id) in [
        ("both set", Some("refs/heads/main"), Some("4242"), Some("refs/heads/main"), Some(4242)),
        ("neither set", None, None, None, None),
        ("both empty", Some(""), Some(""), None, None),
        ("a ref of 256 bytes", Some(long.as_str()), Some("1"), Some(long.as_str()), Some(1)),
        ("a ref of 257 bytes", Some(longer.as_str()), Some("1"), None, Some(1)),
        ("a tab in the ref", Some("refs/heads/a\tb"), Some("1"), None, Some(1)),
        ("a C1 control in the ref", Some("refs/heads/\u{85}"), Some("1"), None, Some(1)),
        ("an expression as the ref", Some("${{ github.ref }}"), Some("1"), Some("${{ github.ref }}"), Some(1)),
        ("an id that is not a number", Some("refs/heads/main"), Some("12a"), Some("refs/heads/main"), None),
        ("a negative id", Some("refs/heads/main"), Some("-1"), Some("refs/heads/main"), None),
        ("an id past 2^53", Some("refs/heads/main"), Some("9007199254740993"), Some("refs/heads/main"), None),
    ] {
        let (port, seen) = front_stand_in(r#"{"verdict":"keep"}"#);
        let mut cmd = std::process::Command::new("sh");
        cmd.arg(&hook)
            .env_clear()
            .env("GITHUB_REPOSITORY", "example-org/app")
            .env("GITHUB_RUN_ID", "1")
            .env("GITHUB_EVENT_NAME", "push")
            .env("HTTP_PROXY", format!("http://127.0.0.1:{port}"));
        if let Some(r) = git_ref {
            cmd.env("GITHUB_REF", r);
        }
        if let Some(i) = id {
            cmd.env("GITHUB_REPOSITORY_ID", i);
        }
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "{why}: {out:?}");
        let asked = seen.lock().unwrap().clone();
        assert_eq!(asked.len(), 1, "{why}: {asked:?}");
        let body: serde_json::Value = serde_json::from_str(asked[0].split("\n\n").last().unwrap()).unwrap();
        assert_eq!(body["ref"].as_str(), want_ref, "{why}: {body}");
        assert_eq!(body["repo_id"].as_u64(), want_id, "{why}: {body}");
        // The wire of pod-job takes the body.
        serde_json::from_value::<podrunner_source::RealJob>(body.clone())
            .unwrap_or_else(|e| panic!("{why}: {e}: {body}"));
    }
    let _ = std::fs::remove_file(&job);
}
