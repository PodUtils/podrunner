//! The official runner runs canned jobs against the fake broker, offline.
//! Ignored by default: it needs an unpacked release of `actions/runner`
//! (v2.338.2, `images/runner/runner.toml`) in a directory it may change.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-broker-fake --test runner -- --ignored

use std::sync::Mutex;
use std::time::Duration;

use podrunner_broker_fake::{Broker, Job};

mod common;

use common::run_once;

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
        broker.queue(Job::scripts("own", &["echo step > marker.txt"]));
        let _ = std::fs::remove_dir_all(dir.join("_work"));
        run_once(&dir, &broker, &[("ACTIONS_RUNNER_HOOK_JOB_STARTED", hook.display().to_string())]);
        std::thread::sleep(Duration::from_millis(100));
        let done = broker.completions();
        assert_eq!(done[0].conclusion, if ok { "succeeded" } else { "failed" }, "{want:?}: {done:?}");
        assert_eq!(dir.join("_work/app/app/marker.txt").exists(), ok, "{want:?}: the step ran or not");
        // T-201: an annotation of the job names the cause of a foreign job.
        assert_eq!(
            broker.logs().contains("##[error]podrunner: this runner was placed"),
            !ok,
            "{want:?}: {}",
            broker.logs()
        );
    }
    let _ = std::fs::remove_file(&job);
}

/// A stand-in of the front of the egress socket on the loopback: it
/// answers each question with `verdict`, and keeps the head and the body.
fn front_stand_in(verdict: &'static str) -> (u16, std::sync::Arc<Mutex<Vec<String>>>) {
    use std::io::{BufRead, BufReader, Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
    let keep = seen.clone();
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            let mut r = BufReader::new(c.try_clone().unwrap());
            let mut head = String::new();
            let mut len = 0;
            loop {
                let mut line = String::new();
                if r.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
                head.push_str(&line);
            }
            let mut body = vec![0; len];
            let _ = r.read_exact(&mut body);
            keep.lock().unwrap().push(format!("{head}\n{}", String::from_utf8_lossy(&body)));
            let _ = (&c).write_all(
                format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{verdict}", verdict.len())
                    .as_bytes(),
            );
        }
    });
    (port, seen)
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
        run_once(&dir, &broker, &env);
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
        assert_eq!(broker.logs().contains("tier-mismatch"), !ok, "{}", broker.logs());
    }
    let _ = std::fs::remove_file(&job);
}
