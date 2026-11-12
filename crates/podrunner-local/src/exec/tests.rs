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
    for n in 1..=2 {
        let e = exec.run(&job, Job::scripts("a", &["true"])).unwrap_err();
        assert!(e.contains(&format!("u-{n}")) && e.contains("no result"), "{e}");
    }
    let unit = podrunner_schema::UnitId::parse("u-1").unwrap();
    assert!(driver.destroy(&unit).unwrap().is_clean(), "destroyed before");
    assert_eq!(exec.logs.len(), 2);
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
    let e = exec.run(&JobId::new("a.0").unwrap(), job).unwrap_err();
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
