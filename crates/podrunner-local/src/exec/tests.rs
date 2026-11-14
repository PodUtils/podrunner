use std::sync::Arc;
use std::time::Duration;

use podrunner_broker_fake::Job;
use podrunner_fake::FakeUnit;
use podrunner_schema::{Capability, JobId};
use podrunner_unit::UnitDriver;

use super::*;
use crate::drive::Executor;

fn capability() -> Capability {
    serde_json::from_value(serde_json::json!({
        "schema": 1, "kind": "fake", "arch": "x86_64", "boundary": "confined", "principal": "distinct",
        "container": "hooks", "net": "broker", "secret_delivery": ["files"], "credential_store": "private-dir",
        "mount_modes": ["rw"], "rootfs_fresh": "yes", "tree_kill": "verified", "limits": {},
        "egress_hold": "advisory", "kvm": "no"
    }))
    .unwrap()
}

/// A top process that is not the runner gives no result: the job fails
/// with its reason, and its unit is destroyed (each unit id is new).
/// The unit `u-N` that an error of the executor names: the count of units
/// is one for the process (T-132).
fn unit_of(e: &str) -> podrunner_schema::UnitId {
    let at = e.find("u-").expect("a unit id");
    let id: String = e[at..].chars().take_while(|c| *c == '-' || *c == 'u' || c.is_ascii_digit()).collect();
    podrunner_schema::UnitId::parse(&id).unwrap()
}

#[test]
fn a_unit_with_no_result_fails_its_job_and_is_destroyed() {
    let dir = std::env::temp_dir().join(format!("podrunner-local-unit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let driver: Arc<dyn UnitDriver> = Arc::new(FakeUnit::new(&dir, capability(), "linux/amd64"));
    let image = driver.prepare(&format!("img@sha256:{}", "a".repeat(64)), "linux/amd64", false).unwrap();
    let mut exec = InUnit::new(driver.clone(), image);
    exec.run_sh = "true".into();
    exec.grace = Duration::ZERO;
    let job = JobId::new("a.0").unwrap();
    let mut ids = Vec::new();
    for _ in 1..=2 {
        let e = exec.run(&job, Job::scripts("a", &["true"]), Duration::from_secs(60)).unwrap_err();
        assert!(e.contains("no result"), "{e}");
        ids.push(unit_of(&e));
    }
    assert_ne!(ids[0], ids[1], "each job gets a new unit");
    assert!(driver.destroy(&ids[0]).unwrap().is_clean(), "destroyed before");
    assert_eq!(exec.logs.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A unit that runs past the limit of its job is ended, and its log is kept.
#[test]
fn a_unit_past_its_limit_is_ended() {
    let dir = std::env::temp_dir().join(format!("podrunner-local-limit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let driver: Arc<dyn UnitDriver> = Arc::new(FakeUnit::new(&dir, capability(), "linux/amd64"));
    let image = driver.prepare(&format!("img@sha256:{}", "a".repeat(64)), "linux/amd64", false).unwrap();
    let mut exec = InUnit::new(driver.clone(), image);
    // A runner that hangs.
    let hang = dir.with_extension("sh");
    std::fs::write(&hang, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&hang, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    exec.run_sh = hang.to_str().unwrap().into();
    exec.grace = Duration::ZERO;
    let t = std::time::Instant::now();
    let e = exec.run(&JobId::new("a.0").unwrap(), Job::scripts("a", &["true"]), Duration::from_millis(300));
    let e = e.unwrap_err();
    assert!(e.contains("timeout-minutes"), "{e}");
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
    assert_eq!(exec.logs.len(), 1);
    assert!(driver.destroy(&unit_of(&e)).unwrap().is_clean());
    let _ = std::fs::remove_file(&hang);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_runner_directory_refuses_a_job_with_containers() {
    let dir = std::env::temp_dir().join(format!("podrunner-local-dir-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("run.sh"), "").unwrap();
    std::fs::write(dir.join("bin/Runner.Listener"), "").unwrap();
    let mut exec = RunnerDir::new(dir.clone()).unwrap();
    let mut job = Job::scripts("a", &["true"]);
    assert!(!needs_containers(&job));
    job.overrides.insert("jobContainer".into(), serde_json::json!("alpine@sha256:00"));
    let e = exec.run(&JobId::new("a.0").unwrap(), job, Duration::from_secs(60)).unwrap_err();
    assert!(e.contains("--podvm"), "{e}");
    let mut docker = Job::scripts("a", &["true"]);
    docker.overrides.insert("steps".into(), serde_json::json!([{ "reference": { "type": "containerRegistry" } }]));
    assert!(needs_containers(&docker));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_registries_of_the_images_of_a_job() {
    let d = "a".repeat(64);
    let mut job = Job::scripts("a", &["true"]);
    job.overrides.insert(
        "jobContainer".into(),
        serde_json::json!({ "type": 2, "map": [{ "Key": "image", "Value": format!("ghcr.io/o/i@sha256:{d}") }] }),
    );
    job.overrides.insert("jobServiceContainers".into(), serde_json::json!({ "type": 2, "map": [
        { "Key": "plain", "Value": format!("redis@sha256:{d}") },
        { "Key": "mapped", "Value": { "type": 2, "map": [{ "Key": "image", "Value": format!("localhost:5000/x@sha256:{d}") }] } },
    ] }));
    job.overrides.insert(
        "steps".into(),
        serde_json::json!([{ "reference": { "type": "containerRegistry", "image": format!("quay.io/q@sha256:{d}") } }]),
    );
    assert_eq!(images(&job).len(), 4);
    assert_eq!(registries(&job), ["docker.io", "ghcr.io", "localhost:5000", "quay.io"]);
    let mut plain = Job::scripts("a", &["true"]);
    plain.overrides.insert("jobContainer".into(), serde_json::json!(format!("alpine@sha256:{d}")));
    assert_eq!(registries(&plain), ["docker.io"]);
}

/// A stand-in runner directory whose `run.sh` runs `script`.
fn stand_in(name: &str, script: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("podrunner-local-stand-in-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("run.sh"), format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(dir.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(dir.join("bin/Runner.Listener"), "").unwrap();
    dir
}

/// T-115: a runner past the limit of its job ends with its whole process
/// group, and the job fails with the limit in the reason.
#[test]
fn a_runner_directory_ends_a_job_at_its_limit() {
    let mark = format!("podrunner-t115-{}", std::process::id());
    let dir = stand_in("limit", &format!("sh -c 'exec sleep 300 # {mark}' & wait"));
    let mut exec = RunnerDir::new(dir.clone()).unwrap();
    let t = Instant::now();
    let e = exec.run(&JobId::new("a.0").unwrap(), Job::scripts("a", &["true"]), Duration::from_secs(1)).unwrap_err();
    assert!(e.contains("limit of 1 s"), "{e}");
    assert!(t.elapsed() < Duration::from_secs(30), "the job ends at its limit");
    let left = std::fs::read_dir("/proc")
        .unwrap()
        .flatten()
        .filter_map(|p| std::fs::read(p.path().join("cmdline")).ok())
        .any(|c| String::from_utf8_lossy(&c).contains(&mark));
    assert!(!left, "no process of the runner is left");
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-115: a directory that the runner files cannot go into fails the job
/// with a reason; the command does not panic.
#[test]
fn a_runner_directory_that_takes_no_file_fails_the_job() {
    let dir = stand_in("files", "exit 0");
    // A directory where a runner file is to go: the write fails.
    std::fs::create_dir_all(dir.join(".runner")).unwrap();
    let mut exec = RunnerDir::new(dir.clone()).unwrap();
    let e = exec.run(&JobId::new("a.0").unwrap(), Job::scripts("a", &["true"]), Duration::from_secs(60)).unwrap_err();
    assert!(e.contains(".runner"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-138: `--export DIR` copies the workspace of a job, also of a job that
/// ended with no result, to `DIR/JOB`.
#[test]
fn a_runner_directory_exports_the_workspace_of_a_job() {
    let dir = stand_in("export", "mkdir -p _work/r/r _work/_temp && echo built > _work/r/r/out && touch _work/_temp/t");
    let to = dir.join("exported");
    let mut exec = RunnerDir::new(dir.clone()).unwrap();
    exec.export = Some(to.clone());
    let job = JobId::new("build.0").unwrap();
    assert!(exec.run(&job, Job::scripts("a", &["true"]), Duration::from_secs(60)).is_err());
    assert_eq!(std::fs::read_to_string(to.join("build.0/r/r/out")).unwrap(), "built\n");
    assert!(!to.join("build.0/_temp").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
