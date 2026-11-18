//! slots, mint, sign and the scale-set sidecar on the fake scale-set
//! service: a fork pull request on the trusted set is withdrawn with
//! `tier-mismatch` and never acquired; a member job runs to its report. A
//! member job that GitHub assigns with no JobAvailable runs too (T-166).
//! Design source test 3 for the sidecar: the credential (the fake service
//! plants a canary in the JIT configuration) reaches the unit, and is in no
//! stderr of the sidecar, no file of the state and no report.
//! Ignored by default: it needs the Go binaries.
//!
//!   PODRUNNER_FAKE_SCALESET=PATH PODRUNNER_SCALESET_SIDECAR=PATH \
//!   cargo test -p podrunner --test scaleset -- --ignored

use std::collections::BTreeMap;
use std::io::BufRead;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pod_core::clock::ManualClock;
use podrunner_fake::FakeUnit;
use podrunner_mint::sign::{Signer, key_from_pem};
use podrunner_mint::{Mint, SidecarSource};
use podrunner_schema::{
    Boundary, Capability, ContainerRoute, CredentialStore, EgressHold, ExitReason, Fact, LimitMode, Principal,
    RefusalCode, RunReport, SecretDelivery, TreeKill,
};
use podrunner_slots::config::{Config, SourceConfig, UnitConfig};
use podrunner_slots::{Slots, Source, UnitKind};

const NOW: u64 = 1_800_000_000;
/// The fake scale-set service plants it in the `.credentials` of each JIT
/// configuration.
const CANARY: &str = "fake-scaleset-canary";
const PIN: &str = "runner@sha256:1111111111111111111111111111111111111111111111111111111111111111";
const SCRIPT: &str = r#"{"scaleSetName":"podrunner","jobs":[
 {"requestId":11,"jobId":"a","owner":"example-org","repo":"app","workflowRunId":101,"event":"push","actor":"alice","actorIsMember":true,"visibility":"private","labels":["podrunner"]},
 {"requestId":12,"jobId":"b","owner":"example-org","repo":"app","workflowRunId":102,"event":"pull_request","actor":"mallory","headIsFork":true,"visibility":"public","labels":["podrunner"]}]}"#;
/// A job that GitHub assigns to the scale set with no JobAvailable, and
/// with runnerRequestId 0 (T-166). It runs alone: GitHub gives an assigned
/// job to any runner of the set, so beside another job it can take the
/// runner of that job (T-175).
const ASSIGNED: &str = r#"{"scaleSetName":"podrunner","jobs":[
 {"requestId":13,"jobId":"c","owner":"example-org","repo":"app","workflowRunId":103,"event":"push","actor":"alice","actorIsMember":true,"visibility":"private","labels":["podrunner"],"assignedOnly":true}]}"#;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// One run of slots, mint, sign and the sidecar on the fake with `script`,
/// until `want` reports.
struct Run {
    dir: std::path::PathBuf,
    reports: Vec<RunReport>,
    record: std::path::PathBuf,
    stderr: std::path::PathBuf,
    signer: Arc<Mutex<Signer>>,
}

impl Run {
    fn by_job(&self, id: &str) -> &RunReport {
        self.reports
            .iter()
            .find(|r| r.job_id.as_str() == id)
            .unwrap_or_else(|| panic!("no report of {id}: {:?}", self.reports))
    }

    fn acquires(&self) -> Vec<String> {
        let text = std::fs::read_to_string(&self.record).unwrap();
        text.lines().filter(|l| l.contains("acquirejobs")).map(String::from).collect()
    }
}

fn run(tag: &str, script: &str, want: usize) -> Run {
    let dir = std::env::temp_dir().join(format!("podrunner-e2e-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("script.json"), script).unwrap();
    let record = dir.join("record.jsonl");
    let mut fake = Command::new(env("PODRUNNER_FAKE_SCALESET"))
        .args(["-listen", "127.0.0.1:0", "-script"])
        .arg(dir.join("script.json"))
        .arg("-record")
        .arg(&record)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut url = String::new();
    std::io::BufReader::new(fake.stdout.take().unwrap()).read_line(&mut url).unwrap();
    let url = url.trim().to_string();

    let clock = Arc::new(ManualClock::new());
    let pem = Command::new("openssl").args(["genrsa", "-traditional", "2048"]).output().unwrap().stdout;
    let key = key_from_pem(&String::from_utf8(pem).unwrap()).unwrap();
    // The sidecar sets iat from the clock of the host: sign reads the same.
    let unix = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let signer = Arc::new(Mutex::new(Signer::new(key, "4242", clock.clone(), Box::new(unix))));
    let args = [
        "-config-url",
        &format!("{url}/example-org"),
        "-api-url",
        &format!("{url}/api/v3"),
        "-installation-id",
        "7",
        "-scale-set",
        "podrunner",
        "-source",
        "org-trusted",
        "-arch",
        "x86_64",
        "-app-id",
        "4242",
        "-operator-controlled",
    ]
    .map(String::from)
    .to_vec();
    // A wrapper keeps the stderr of the sidecar for the scan; descriptor 3
    // (sign) passes through exec.
    let wrapper = dir.join("sidecar.sh");
    let stderr = dir.join("sidecar.err");
    std::fs::write(
        &wrapper,
        format!("#!/bin/sh\nexec '{}' \"$@\" 2>>'{}'\n", env("PODRUNNER_SCALESET_SIDECAR"), stderr.display()),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let sidecar = SidecarSource::new(wrapper.to_str().unwrap(), args, signer.clone(), "sidecar:org-trusted");
    let source_cfg: SourceConfig = serde_json::from_value(serde_json::json!({
        "name": "org-trusted", "kind": "github-scaleset", "tier": "trusted", "arch": "x86_64", "operator_controlled": true
    }))
    .unwrap();
    let mut cfg = Config::parse(&format!(
        "host = \"host-a\"\nallow_fake = true\n[trust]\norganizations = [{{ login = \"example-org\" }}]\n[pin]\n[[pin.payload]]\narch = \"x86_64\"\nreference = \"{PIN}\"\nbuilt_at_s = {NOW}\n"
    ))
    .unwrap();
    cfg.source.push(source_cfg.clone());
    let cap = Capability {
        schema: 1,
        kind: "fake".into(),
        arch: "x86_64".into(),
        boundary: Boundary::Confined,
        boundary_proof: None,
        principal: Principal::Distinct,
        container: ContainerRoute::None,
        net: "broker".into(),
        secret_delivery: vec![SecretDelivery::Files],
        credential_store: CredentialStore::PrivateDir,
        mount_modes: vec!["rw".into()],
        rootfs_fresh: Fact::Yes,
        tree_kill: TreeKill::Verified,
        limits: BTreeMap::from([("wall".into(), LimitMode::Enforced)]),
        egress_hold: EgressHold::Advisory,
        kvm: Fact::No,
    };
    let unit_cfg: UnitConfig = serde_json::from_value(
        serde_json::json!({ "slots": 2, "command": ["sh", "-c", &format!("grep -q {CANARY} .runner-root/.credentials && sleep 1")] }),
    )
    .unwrap();
    cfg.unit.insert("fake".into(), unit_cfg.clone());
    let kinds = vec![UnitKind {
        name: "fake".into(),
        cfg: unit_cfg,
        driver: Arc::new(FakeUnit::new(&dir.join("fake"), cap.clone(), "linux/amd64")),
        capability: cap,
    }];
    let sources =
        vec![Source { cfg: source_cfg, source: Box::new(Mint::new("org-trusted", Box::new(sidecar))), revoked: false }];
    let mut slots = Slots::start(cfg, &dir.join("state"), clock.clone(), Box::new(|| NOW), sources, kinds).unwrap();

    for _ in 0..200 {
        slots.tick();
        clock.advance(Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(20));
        if slots.reports.len() == want {
            break;
        }
    }
    let _ = fake.kill();
    let _ = fake.wait();
    Run { dir, reports: slots.reports.iter().cloned().collect(), record, stderr, signer }
}

#[test]
#[ignore]
fn a_fork_is_withdrawn_and_a_member_job_runs() {
    let r = run("fork", SCRIPT, 2);
    assert_eq!(r.by_job("12").refusal_code, Some(RefusalCode::TierMismatch));
    let member = r.by_job("11");
    assert_eq!(member.exit_reason, ExitReason::Success, "{member:?}");
    let acquired = r.acquires();
    assert_eq!(acquired.len(), 1, "one acquire: {acquired:?}");
    assert!(r.signer.lock().unwrap().audit.iter().all(|a| a.outcome == "signed"));
    // The member job succeeded, so its unit saw the canary: the positive
    // control. The stderr file exists, so the scan reads it.
    let err = std::fs::read_to_string(&r.stderr).expect("the stderr of the sidecar");
    assert!(!err.contains(CANARY), "the stderr of the sidecar holds the credential");
    assert!(!serde_json::to_string(&r.reports).unwrap().contains(CANARY), "a report holds the credential");
    for f in ["state/journal.jsonl", "state/audit.jsonl"] {
        let text = std::fs::read_to_string(r.dir.join(f)).unwrap();
        assert!(!text.is_empty() && !text.contains(CANARY), "{f} holds the credential");
    }
    let _ = std::fs::remove_dir_all(&r.dir);
}

/// T-166: github.com assigns the job with no JobAvailable. It is offered as
/// `a-JOBID`, claimed with no acquire, minted, and runs to its report.
#[test]
#[ignore]
fn an_assigned_job_runs_with_no_acquire() {
    let r = run("assigned", ASSIGNED, 1);
    let job = r.by_job("a-c");
    assert_eq!(job.exit_reason, ExitReason::Success, "{job:?}");
    assert!(r.acquires().is_empty(), "{:?}", r.acquires());
    let _ = std::fs::remove_dir_all(&r.dir);
}
