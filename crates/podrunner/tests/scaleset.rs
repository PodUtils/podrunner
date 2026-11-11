//! slots, mint, sign and the scale-set sidecar on the fake scale-set
//! service: a fork pull request on the trusted set is withdrawn with
//! `tier-mismatch` and never acquired; a member job runs to its report.
//! Ignored by default: it needs the Go binaries.
//!
//!   PODRUNNER_FAKE_SCALESET=PATH PODRUNNER_SCALESET_SIDECAR=PATH \
//!   cargo test -p podrunner --test scaleset -- --ignored

use std::collections::BTreeMap;
use std::io::BufRead;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use podrunner_clock::ManualClock;
use podrunner_fake::FakeUnit;
use podrunner_mint::sign::{Signer, key_from_pem};
use podrunner_mint::{Mint, SidecarSource};
use podrunner_schema::{
    Boundary, Capability, ContainerRoute, CredentialStore, EgressHold, ExitReason, Fact, LimitMode, Principal,
    RefusalCode, SecretDelivery, TreeKill,
};
use podrunner_slots::config::{Config, SourceConfig, UnitConfig};
use podrunner_slots::{Slots, Source, UnitKind};

const NOW: u64 = 1_800_000_000;
const PIN: &str = "runner@sha256:1111111111111111111111111111111111111111111111111111111111111111";
const SCRIPT: &str = r#"{"scaleSetName":"podrunner","jobs":[
 {"requestId":11,"jobId":"a","owner":"example-org","repo":"app","workflowRunId":101,"event":"push","actor":"alice","actorIsMember":true,"visibility":"private","labels":["podrunner"]},
 {"requestId":12,"jobId":"b","owner":"example-org","repo":"app","workflowRunId":102,"event":"pull_request","actor":"mallory","headIsFork":true,"visibility":"public","labels":["podrunner"]}]}"#;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[test]
#[ignore]
fn a_fork_is_withdrawn_and_a_member_job_runs() {
    let dir = std::env::temp_dir().join(format!("podrunner-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("script.json"), SCRIPT).unwrap();
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

    let clock = ManualClock::default();
    let pem = Command::new("openssl").args(["genrsa", "-traditional", "2048"]).output().unwrap().stdout;
    let key = key_from_pem(&String::from_utf8(pem).unwrap()).unwrap();
    // The sidecar sets iat from the clock of the host: sign reads the same.
    let unix = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let signer = Arc::new(Mutex::new(Signer::new(key, "4242", Arc::new(clock.clone()), Box::new(unix))));
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
    let sidecar = SidecarSource::new(&env("PODRUNNER_SCALESET_SIDECAR"), args, signer.clone(), "sidecar:org-trusted");
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
        serde_json::json!({ "slots": 2, "command": ["sh", "-c", "test -f .runner-root/.runner && sleep 1"] }),
    )
    .unwrap();
    cfg.unit.insert("fake".into(), unit_cfg.clone());
    let kinds = vec![UnitKind {
        name: "fake".into(),
        cfg: unit_cfg,
        driver: Box::new(FakeUnit::new(&dir.join("fake"), cap.clone(), "linux/amd64")),
        capability: cap,
    }];
    let sources =
        vec![Source { cfg: source_cfg, source: Box::new(Mint::new("org-trusted", Box::new(sidecar))), revoked: false }];
    let mut slots =
        Slots::start(cfg, &dir.join("state"), Arc::new(clock.clone()), Box::new(|| NOW), sources, kinds).unwrap();

    for _ in 0..200 {
        slots.tick();
        clock.advance(Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(20));
        if slots.reports.len() == 2 {
            break;
        }
    }
    let _ = fake.kill();
    let _ = fake.wait();
    let by_job = |id: &str| {
        slots
            .reports
            .iter()
            .find(|r| r.job_id.as_str() == id)
            .unwrap_or_else(|| panic!("no report of {id}: {:?}", slots.reports))
    };
    assert_eq!(by_job("12").refusal_code, Some(RefusalCode::TierMismatch));
    let member = by_job("11");
    assert_eq!(member.exit_reason, ExitReason::Success, "{member:?}");
    let acquired: Vec<String> = std::fs::read_to_string(&record)
        .unwrap()
        .lines()
        .filter(|l| l.contains("acquirejobs"))
        .map(String::from)
        .collect();
    assert_eq!(acquired.len(), 1, "one acquire: {acquired:?}");
    assert!(signer.lock().unwrap().audit.iter().all(|a| a.outcome == "signed"));
    let _ = std::fs::remove_dir_all(&dir);
}
