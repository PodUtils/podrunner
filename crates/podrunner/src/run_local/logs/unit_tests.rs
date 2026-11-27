//! T-723: `--log-dir` keeps the lines that podrunner adds to the log of a
//! job in a unit, after the lines of the runner, each with a mark.

use std::sync::Arc;
use std::time::Duration;

use podrunner_local::drive::Executor;
use podrunner_local::exec::InUnit;
use podrunner_schema::JobId;
use podrunner_unit::UnitDriver;

fn capability() -> podrunner_schema::Capability {
    serde_json::from_value(serde_json::json!({
        "schema": 1, "kind": "fake", "arch": "x86_64", "boundary": "confined", "principal": "distinct",
        "container": "hooks", "net": "broker", "secret_delivery": ["files"], "credential_store": "private-dir",
        "mount_modes": ["rw"], "rootfs_fresh": "yes", "tree_kill": "verified", "limits": {},
        "egress_hold": "advisory", "kvm": "no"
    }))
    .unwrap()
}

/// A job that fails in a fake unit: its `job.log` has the lines of
/// podrunner, and the made-up secret of the job is masked in them.
#[test]
fn a_failed_job_in_a_unit_keeps_the_lines_of_podrunner() {
    let dir = std::env::temp_dir().join(format!("podrunner-t723-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let driver: Arc<dyn UnitDriver> =
        Arc::new(podrunner_fake::FakeUnit::new(&dir.join("units"), capability(), "linux/amd64"));
    let image = driver.prepare(&format!("img@sha256:{}", "a".repeat(64)), "linux/amd64", false).unwrap();
    let mut exec = InUnit::new(driver, image);
    // The top process writes a line and gives no result.
    exec.run_sh = "echo made-up-t723-secret".into();
    exec.grace = Duration::ZERO;
    let job = JobId::new("a.0").unwrap();
    let e = exec.run(&job, podrunner_local::Job::scripts("a", &["true"]), Duration::from_secs(60)).unwrap_err();
    assert!(e.contains("no result"), "{e}");
    let (blobs, logs) = (exec.take_blobs(), exec.take_logs());
    super::write_dir(&dir.join("logs"), &blobs, &logs, &["made-up-t723-secret".into()]).unwrap();
    let text = std::fs::read_to_string(dir.join("logs/a.0/job.log")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(text.contains("podrunner: the last requests of the broker"), "job.log:\n{text}");
    assert!(!text.contains("made-up-t723-secret"), "job.log:\n{text}");
    assert!(text.lines().all(|l| l.starts_with(MARK)), "each line has the mark:\n{text}");
}

const MARK: &str = "[podrunner] ";

/// The lines of podrunner come after the text of the runner, each with
/// the mark; an empty entry adds no line.
#[test]
fn the_lines_of_podrunner_come_after_the_lines_of_the_runner() {
    let dir = std::env::temp_dir().join(format!("podrunner-t723-order-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let added = podrunner_local::exec::ADDED.to_string();
    let blobs = vec![
        (
            JobId::new("a.0").unwrap(),
            vec![
                (added.clone(), "\npodrunner: one\nthe end of the output\n\npodrunner: two".to_string()),
                ("job".to_string(), "runner line".to_string()),
            ],
        ),
        (JobId::new("b.0").unwrap(), vec![(added, String::new())]),
    ];
    super::write_dir(&dir, &blobs, &[], &[]).unwrap();
    let text = std::fs::read_to_string(dir.join("a.0/job.log")).unwrap();
    let empty = dir.join("b.0/job.log").exists();
    let _ = std::fs::remove_dir_all(&dir);
    let want = "runner line\n[podrunner] podrunner: one\n[podrunner] the end of the output\n[podrunner] \n\
                [podrunner] podrunner: two\n";
    assert_eq!(text, want);
    assert!(!empty, "an empty entry writes no job.log");
}
