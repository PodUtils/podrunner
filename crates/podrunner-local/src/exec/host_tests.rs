//! The end of a run on a runner directory (T-308): no runner file and no
//! work directory stay, a configured runner is refused, and `doctor --run`
//! sees a directory that a run holds.

use super::*;

/// A stand-in runner in a new directory `name`: `run.sh` runs `script`.
fn stand_in(name: &str, script: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("podrunner-host-end-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("run.sh"), format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(dir.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(dir.join("bin/Runner.Listener"), "").unwrap();
    dir
}

/// A20 and A19: a runner that ends with no cleanup (as at a signal) leaves
/// no runner file after its job, and the work directory goes with the run.
#[test]
fn a_run_leaves_no_runner_file_and_no_work_directory() {
    let dir = stand_in("files", "test -f .credentials_rsaparams && mkdir -p \"$W/x\" && echo data > \"$W/x/f\"");
    let mut exec = RunnerDir::new(dir.clone()).unwrap();
    let work = exec.work.clone();
    let script = std::fs::read_to_string(dir.join("run.sh")).unwrap().replace("$W", &work.display().to_string());
    std::fs::write(dir.join("run.sh"), script).unwrap();
    let job = JobId::new("a.0").unwrap();
    // The stand-in sends no result: the job fails, after its runner ran.
    let e = exec.run(&job, Job::scripts("a", &["true"]), Duration::from_secs(60)).unwrap_err();
    assert!(e.contains("no result"), "{e}");
    assert!(work.join("x/f").is_file(), "the runner ran and wrote into the work directory");
    for f in RUNNER_FILES {
        assert!(!dir.join(f).exists(), "{f} stays after the job");
    }
    drop(exec);
    assert!(!work.exists(), "{} stays after the run", work.display());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A runner that `config.sh` configured is refused by name; a `.runner`
/// that podrunner left is not.
#[test]
fn a_configured_runner_is_refused() {
    let dir = stand_in("configured", "exit 0");
    std::fs::write(dir.join(".runner"), r#"{"agentId":3,"poolName":"Default","serverUrl":"https://x.example/"}"#)
        .unwrap();
    let e = RunnerDir::new(dir.clone()).err().expect("a refusal");
    assert!(e.contains("a configured runner (pool \"Default\")") && e.contains("fix:"), "{e}");
    let ours = podrunner_broker_fake::files::runner_files("http://127.0.0.1:1/", "runner-u-1", "k");
    std::fs::write(dir.join(".runner"), &ours[0].1).unwrap();
    assert!(RunnerDir::new(dir.clone()).is_ok(), "the .runner of podrunner");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A28: a directory that a run holds is busy; a free one, and one with no
/// lock file, is not, and the check writes no file.
#[test]
fn a_held_runner_directory_is_busy() {
    let dir = stand_in("busy", "exit 0");
    assert_eq!(busy(&dir), None);
    assert!(!dir.join(LOCK).exists(), "the check writes no lock file");
    let held = lock(&dir).unwrap();
    let e = busy(&dir).expect("busy");
    assert!(e.contains("another run of podrunner"), "{e}");
    drop(held);
    let _ = std::fs::remove_dir_all(&dir);
}
