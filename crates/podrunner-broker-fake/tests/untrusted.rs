//! The acceptance of GitHub #37 (T-037): a fork pull request offer on the
//! untrusted set runs a `run:` job in a guest of the microvm kind, through
//! slots, and a `container:` need on the same set is
//! `container-not-supported`. The kind gets its claim of `guest` from the
//! self-test of GitHub #36 on this host. The fake source gives the offers;
//! its `mint` gives the files of the fake broker, which the guest reaches
//! only through the egress socket of its unit. Ignored by default: it needs
//! podvm, QEMU and a store with the runner image.
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_RUNNER_IMAGE=NAME@sha256:HEX \
//!   cargo test -p podrunner-broker-fake --test untrusted -- --ignored --nocapture

mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use podrunner_broker_fake::{Broker, Job, files::runner_files};
use podrunner_clock::{Clock, SystemClock};
use podrunner_fake::{FakeSource, parse_script};
use podrunner_microvm::MicrovmUnit;
use podrunner_podvm::Podvm;
use podrunner_schema::{Boundary, Credential, CredentialFile, ExitReason, JobId, RefusalCode, Secret, Tier};
use podrunner_slots::config::{Config, SourceConfig, UnitConfig};
use podrunner_slots::{EgressPort, Slots, Source, UnitKind};
use podrunner_source::{Event, Hello, JobSource, SourceError};
use podrunner_unit::UnitDriver;
use serde_json::{Map, Value, json};

/// The name of the broker in the guest.
const NAME: &str = "broker.podrunner.test";

/// The fake source, with the files of the fake broker as its credential.
struct BrokerSource {
    inner: FakeSource,
    base: String,
}

impl JobSource for BrokerSource {
    fn hello(&mut self, client: &str, version: &str) -> Result<Hello, SourceError> {
        self.inner.hello(client, version)
    }
    fn next(&mut self, capacity: u32, wait: Duration) -> Result<Vec<Event>, SourceError> {
        self.inner.next(capacity, wait)
    }
    fn ack(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.inner.ack(job)
    }
    fn claim(&mut self, job: &JobId, runner_name: &str) -> Result<(), SourceError> {
        self.inner.claim(job, runner_name)
    }
    fn mint(&mut self, job: &JobId, runner_name: &str) -> Result<Credential, SourceError> {
        self.inner.mint(job, runner_name)?;
        let files = runner_files(&self.base, runner_name, &common::rsaparams())
            .into_iter()
            .map(|(name, text)| CredentialFile { name, content: Secret::new(text) })
            .collect();
        Ok(Credential { files })
    }
    fn report(&mut self, job: &JobId, report: &Map<String, Value>) -> Result<(), SourceError> {
        self.inner.report(job, report)
    }
    fn audit(&mut self, line: &Map<String, Value>) -> Result<(), SourceError> {
        self.inner.audit(line)
    }
    fn withdraw(&mut self, job: &JobId, reason: &str) -> Result<(), SourceError> {
        self.inner.withdraw(job, reason)
    }
    fn release(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.inner.release(job)
    }
}

/// The egress module of the test: each unit socket is a proxy to the
/// broker, and the log of its request lines.
struct ToBroker {
    port: u16,
    seen: Arc<Mutex<Vec<String>>>,
}

impl EgressPort for ToBroker {
    fn open(&self, _unit: &str, _policy: &str, socket: &Path) -> Result<usize, String> {
        let log = common::proxy(socket, self.port);
        let seen = self.seen.clone();
        std::thread::spawn(move || {
            for _ in 0..1200 {
                std::thread::sleep(Duration::from_millis(500));
                let mut s = seen.lock().unwrap();
                for l in log.lock().unwrap().drain(..) {
                    s.push(l);
                }
            }
        });
        Ok(0)
    }
    fn close(&self, _unit: &str) {}
    fn alive(&self) -> bool {
        true
    }
}

/// An offer of a fork pull request, with the fields of `with` over it.
fn fork_offer(job: &str, with: Value) -> String {
    let mut o = json!({
        "schema": 1, "job_id": job, "source": "forks", "labels": ["podrunner"], "arch": "x86_64",
        "wait_deadline_s": 600,
        "trust_inputs": {
            "event": "pull_request", "head_is_fork": "yes", "actor_class": "outside",
            "repo_visibility": "public", "owner": "example-org", "repo": "app", "run_id": 1,
            "ref": "refs/pull/1/merge", "workflow_ref": "w"
        }
    });
    for (k, v) in with.as_object().unwrap() {
        o[k] = v.clone();
    }
    json!({ "at_s": 0, "offer": o }).to_string()
}

#[test]
#[ignore]
fn a_fork_pull_request_runs_in_a_guest_and_a_container_need_is_refused() {
    let image = common::env("PODRUNNER_RUNNER_IMAGE");
    let state = std::env::temp_dir().join(format!("podrunner-untrusted-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(&state).unwrap();
    let now_unix = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();

    let broker = Broker::start_seen_at(NAME).unwrap();
    broker.queue(Job::scripts("fork-job", &["echo podrunner-fork-$((6*7))"]));
    let port: u16 = broker.base.trim_end_matches('/').rsplit(':').next().unwrap().parse().unwrap();

    let mut cfg = Config::parse(&format!(
        r#"
host = "host-a"
allow_fake = true
guest_kinds = ["microvm"]
[deadlines]
start_s = 900
[trust]
organizations = [{{ login = "example-org" }}]
[pin]
[[pin.payload]]
arch = "x86_64"
reference = "{image}"
built_at_s = {now}
"#,
        now = now_unix()
    ))
    .unwrap();

    // The untrusted set and its two offers.
    let sc: SourceConfig = serde_json::from_value(json!({
        "name": "forks", "kind": "fake", "tier": "untrusted", "arch": "x86_64", "operator_controlled": true
    }))
    .unwrap();
    cfg.source.push(sc.clone());
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let lines = [
        fork_offer("fork-run", json!({})),
        fork_offer("fork-container", json!({ "unit_need": { "container": true } })),
    ];
    let script = parse_script(&lines.join("\n")).unwrap();
    let source = BrokerSource { inner: FakeSource::new("forks", clock.clone(), script), base: broker.base.clone() };

    // The microvm kind, with the claim that the self-test on this host gives.
    let unit = MicrovmUnit::new(
        Podvm { binary: PathBuf::from(common::env("PODRUNNER_PODVM")) },
        &state.join("microvm"),
        &PathBuf::from(common::env("PODRUNNER_PODVM_IMAGES")),
    );
    let verdict =
        podrunner_slots::guest::self_test(&unit, &image, "linux/amd64", podrunner_slots::guest::test_ids(), &state);
    let (capability, why) = podrunner_slots::guest::settle(unit.probe().unwrap(), true, verdict);
    assert_eq!(capability.boundary, Boundary::Guest, "the self-test of the guest: {why:?}");
    let unit_cfg: UnitConfig =
        serde_json::from_value(json!({ "slots": 1, "command": ["/home/runner/run.sh"] })).unwrap();
    cfg.unit.insert("microvm".into(), unit_cfg.clone());
    let kinds = vec![UnitKind { name: "microvm".into(), cfg: unit_cfg, driver: Arc::new(unit), capability }];

    let seen = Arc::new(Mutex::new(Vec::new()));
    let sources = vec![Source { cfg: sc, source: Box::new(source), revoked: false }];
    let mut slots = Slots::start(cfg, &state.join("state"), clock, Box::new(now_unix), sources, kinds)
        .unwrap()
        .with_egress(Box::new(ToBroker { port, seen: seen.clone() }));

    let end = Instant::now() + Duration::from_secs(900);
    while Instant::now() < end && slots.reports.len() != 2 {
        slots.tick();
        std::thread::sleep(Duration::from_millis(500));
    }
    let reports: Vec<_> = slots.reports.iter().cloned().collect();
    let through = seen.lock().unwrap().clone();
    println!("{reports:#?}\nthrough the egress socket: {through:?}\nthe broker saw: {:?}", broker.seen());
    let _ = std::fs::remove_dir_all(&state);

    let run = reports.iter().find(|r| r.job_id.as_str() == "fork-run").expect("a report of the fork job");
    assert_eq!(run.tier, Some(Tier::Untrusted));
    assert_eq!(run.exit_reason, ExitReason::Success, "{run:?}");
    let u = run.unit.as_ref().expect("the unit of the fork job");
    assert_eq!((u.unit_kind.as_str(), u.boundary), ("microvm", Boundary::Guest));
    let done = broker.completions();
    assert_eq!(done.len(), 1, "{:?}", broker.seen());
    assert_eq!(done[0].conclusion, "succeeded", "{}", broker.logs());
    assert!(broker.logs().contains("podrunner-fork-42"), "{}", broker.logs());
    assert!(through.iter().any(|l| l.contains(NAME)), "the guest reached the broker through its egress socket");

    let refused = reports.iter().find(|r| r.job_id.as_str() == "fork-container").expect("a report of the need");
    assert_eq!(refused.refusal_code, Some(RefusalCode::ContainerNotSupported), "{refused:?}");
}
