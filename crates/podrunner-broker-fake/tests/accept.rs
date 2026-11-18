//! The checks C1 to C5 of GitHub #292 (T-525) on a real kind, through the
//! slots of `serve`: 20 jobs in a burst, each in a fresh unit (C1); `busy`
//! never over `slots` (C2); a cancel ends its unit within 60 s, and then
//! `busy` is 0 and `degraded` is null (C3); each sample of `status` goes to
//! a file that the workflow validates against `status.v1` of `pod-job`
//! (C4); a job finds no canary of the host and no `key_file` (C5).
//!
//! The kind is `PODRUNNER_ACCEPT_KIND`: `microvm` (a guest, fork offers at
//! `untrusted`) or `incus` (a system container, member offers at
//! `trusted`). The fake source gives the offers; its `mint` gives the files
//! of the fake broker, which a unit reaches only through its egress socket.
//! Ignored by default: it needs podvm, a store with the runner image, and
//! QEMU (microvm) or incusd (incus).
//!
//!   PODRUNNER_ACCEPT_KIND=microvm PODRUNNER_PODVM=PATH \
//!   PODRUNNER_PODVM_IMAGES=STORE PODRUNNER_RUNNER_IMAGE=NAME@sha256:HEX \
//!   PODRUNNER_ACCEPT_OUT=FILE \
//!   cargo test -p podrunner-broker-fake --test accept -- --ignored --nocapture

mod common;

use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use pod_core::clock::{Clock, SystemClock};
use podrunner_broker_fake::{Broker, Job};
use podrunner_fake::{FakeSource, parse_script};
use podrunner_podvm::Podvm;
use podrunner_schema::{Capability, Credential, CredentialFile, ExitReason, JobId, Residual, Secret};
use podrunner_slots::config::{Config, SourceConfig, UnitConfig};
use podrunner_slots::{Slots, Source, UnitKind};
use podrunner_source::{Event, Hello, JobSource, SourceError};
use podrunner_unit::UnitDriver;
use serde_json::{Map, Value, json};

/// The name of the broker in a unit.
const NAME: &str = "broker.podrunner.test";
/// The jobs of C1 and C2.
const JOBS: usize = 20;
/// The slots of the kind: the burst of 20 waits for them (C2).
const SLOTS: u32 = 2;
/// The time from a cancel to the report of its destroyed unit (C3).
const CANCEL_BOUND: Duration = Duration::from_secs(60);
/// The job that C3 cancels.
const LONG: &str = "c3-long";

/// The fake source, with the files of the fake broker as its credential.
/// It holds the offer of [`LONG`] until `release` (the broker gives its
/// jobs in order, to any runner), and cancels it 60 s after its mint.
struct BrokerSource {
    inner: FakeSource,
    broker: Arc<Broker>,
    held: Option<Event>,
    release: Arc<AtomicBool>,
    long_minted: Option<Instant>,
    cancelled: Arc<std::sync::Mutex<Option<Instant>>>,
}

impl JobSource for BrokerSource {
    fn hello(&mut self, client: &str, version: &str) -> Result<Hello, SourceError> {
        self.inner.hello(client, version)
    }
    fn next(&mut self, capacity: u32, wait: Duration) -> Result<Vec<Event>, SourceError> {
        let mut events = self.inner.next(capacity, wait)?;
        if let Some(i) = events.iter().position(|e| matches!(e, Event::Offer { .. }) && e.job_id().as_str() == LONG) {
            self.held = Some(events.remove(i));
        }
        if self.release.load(Ordering::SeqCst) {
            events.extend(self.held.take());
        }
        let mut cancelled = self.cancelled.lock().unwrap();
        if cancelled.is_none() && self.long_minted.is_some_and(|t| t.elapsed() > Duration::from_secs(60)) {
            *cancelled = Some(Instant::now());
            events.push(Event::Cancel { job_id: JobId::new(LONG).unwrap() });
        }
        Ok(events)
    }
    fn ack(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.inner.ack(job)
    }
    fn claim(&mut self, job: &JobId, runner_name: &str) -> Result<(), SourceError> {
        self.inner.claim(job, runner_name)
    }
    fn mint(&mut self, job: &JobId, runner_name: &str) -> Result<Credential, SourceError> {
        self.inner.mint(job, runner_name)?;
        if job.as_str() == LONG {
            self.long_minted = Some(Instant::now());
        }
        let files = self
            .broker
            .runner_files(runner_name)
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

/// An offer of the set: a fork pull request (untrusted) or a push of a
/// member (trusted).
fn offer(job: &str, untrusted: bool) -> String {
    // A trusted job of a public repository needs `trusted_public`: the
    // trusted offer is of a private one.
    let (event, fork, actor, visibility) =
        if untrusted { ("pull_request", "yes", "outside", "public") } else { ("push", "no", "member", "private") };
    let o = json!({
        "schema": 1, "job_id": job, "source": "accept", "labels": ["podrunner"], "arch": std::env::consts::ARCH,
        "wait_deadline_s": 3600,
        "trust_inputs": {
            "event": event, "head_is_fork": fork, "actor_class": actor,
            "repo_visibility": visibility, "owner": "example-org", "repo": "app", "run_id": 1,
            "ref": "refs/heads/main", "workflow_ref": "w"
        }
    });
    json!({ "at_s": 0, "offer": o }).to_string()
}

/// The step of each job of C1 and C5: a marker that no earlier job left,
/// and no canary of the host. The canary and the path come in halves, so
/// that no command line of the unit holds them whole.
fn step(canary: &str, key_file: &str) -> String {
    let (c1, c2) = canary.split_at(canary.len() / 2);
    let (k1, k2) = key_file.split_at(key_file.len() / 2);
    format!(
        "set -e\n\
         m=\"$HOME/.podrunner-c1\"; test ! -e \"$m\" || {{ echo C1: a marker of an earlier job; exit 1; }}; touch \"$m\"\n\
         a='{c1}'; b='{c2}'; printf '%s%s\\n' \"$a\" \"$b\" > /tmp/c5.pat\n\
         k='{k1}'; l='{k2}'; printf '%s%s\\n' \"$k\" \"$l\" >> /tmp/c5.pat\n\
         if test -e \"$k$l\"; then echo C5: the key file is here; exit 1; fi\n\
         for f in /proc/[0-9]*/environ /proc/[0-9]*/cmdline; do\n\
           if {{ tr '\\0' '\\n' < \"$f\"; }} 2>/dev/null | grep -qFf /tmp/c5.pat; then echo \"C5: a canary in $f\"; exit 1; fi\n\
         done\n\
         if grep -rqsFf /tmp/c5.pat /etc /root 2>/dev/null; then echo C5: a canary in a file; exit 1; fi\n\
         echo podrunner-accept-ok"
    )
}

/// The kind of `PODRUNNER_ACCEPT_KIND`, with its claim on this host.
fn kind(name: &str, state: &std::path::Path, image: &str) -> (Arc<dyn UnitDriver>, Capability, bool) {
    let podvm = Podvm { binary: PathBuf::from(common::env("PODRUNNER_PODVM")) };
    let images = PathBuf::from(common::env("PODRUNNER_PODVM_IMAGES"));
    match name {
        "microvm" => {
            let unit = podrunner_microvm::MicrovmUnit::new(podvm, &state.join("microvm"), &images);
            let platform = if std::env::consts::ARCH == "aarch64" { "linux/arm64" } else { "linux/amd64" };
            let ids = podrunner_slots::guest::test_ids();
            let verdict = podrunner_slots::guest::self_test(&unit, image, platform, ids, state);
            let (capability, why) = podrunner_slots::guest::settle(unit.probe().unwrap(), true, verdict);
            assert_eq!(capability.boundary, podrunner_schema::Boundary::Guest, "the self-test: {why:?}");
            (Arc::new(unit), capability, true)
        }
        "incus" => {
            let unit = podrunner_incus::IncusUnit::new(podvm, &state.join("incus"), &images);
            let capability = unit.probe().unwrap();
            (Arc::new(unit), capability, false)
        }
        other => panic!("PODRUNNER_ACCEPT_KIND is microvm or incus, not {other:?}"),
    }
}

#[test]
#[ignore]
fn c1_to_c5_hold_through_the_slots_on_a_real_kind() {
    let kind_name = common::env("PODRUNNER_ACCEPT_KIND");
    let image = common::env("PODRUNNER_RUNNER_IMAGE");
    let state = std::env::temp_dir().join(format!("podrunner-accept-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(&state).unwrap();
    let now_unix = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();

    // C5: a key file of the source and a canary in the environment of the
    // host process (podvm and its helpers inherit it).
    let canary = format!("podrunner-canary-{}-{}", std::process::id(), now_unix());
    let key_file = state.join("source-key.pem");
    std::fs::write(&key_file, &canary).unwrap();
    // SAFETY: one thread runs here, before each other thread of the test.
    unsafe { std::env::set_var("PODRUNNER_C5_CANARY", &canary) };

    let broker = Arc::new(Broker::start_seen_at(NAME).unwrap());
    let port: u16 = broker.base.trim_end_matches('/').rsplit(':').next().unwrap().parse().unwrap();
    let step = step(&canary, &key_file.to_string_lossy());
    for i in 0..JOBS {
        broker.queue(Job::scripts(&format!("c1-{i}"), &[step.as_str()]));
    }

    let (driver, capability, untrusted) = kind(&kind_name, &state, &image);
    let (tier, guest_kinds) =
        if untrusted { ("untrusted", format!("[\"{kind_name}\"]")) } else { ("trusted", "[]".into()) };
    let mut cfg = Config::parse(&format!(
        r#"
host = "host-a"
allow_fake = true
guest_kinds = {guest_kinds}
[deadlines]
start_s = 900
[trust]
organizations = [{{ login = "example-org" }}]
[pin]
[[pin.payload]]
arch = "{arch}"
reference = "{image}"
built_at_s = {now}
"#,
        arch = std::env::consts::ARCH,
        now = now_unix()
    ))
    .unwrap();
    let sc: SourceConfig = serde_json::from_value(json!({
        "name": "accept", "kind": "fake", "tier": tier, "arch": std::env::consts::ARCH,
        "operator_controlled": true, "key_file": key_file
    }))
    .unwrap();
    cfg.source.push(sc.clone());
    let unit_cfg: UnitConfig =
        serde_json::from_value(json!({ "slots": SLOTS, "command": ["/home/runner/run.sh"] })).unwrap();
    cfg.unit.insert(kind_name.clone(), unit_cfg.clone());

    // The burst: each offer at the start; the source holds the long job of
    // C3 until the jobs of C1 end.
    let mut lines: Vec<String> = (0..JOBS).map(|i| offer(&format!("c1-{i}"), untrusted)).collect();
    lines.push(offer(LONG, untrusted));
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let cancelled = Arc::new(std::sync::Mutex::new(None));
    let release = Arc::new(AtomicBool::new(false));
    let source = BrokerSource {
        inner: FakeSource::new("accept", clock.clone(), parse_script(&lines.join("\n")).unwrap()),
        broker: broker.clone(),
        held: None,
        release: release.clone(),
        long_minted: None,
        cancelled: cancelled.clone(),
    };
    let kinds = vec![UnitKind { name: kind_name.clone(), cfg: unit_cfg, driver, capability }];
    let sources = vec![Source { cfg: sc, source: Box::new(source), revoked: false }];
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut slots = Slots::start(cfg, &state.join("state"), clock, Box::new(now_unix), sources, kinds)
        .unwrap()
        .with_egress(Box::new(common::ToBroker { port, seen: seen.clone() }));

    // C2 and C4: a sample of `status` at each tick.
    let out = std::env::var("PODRUNNER_ACCEPT_OUT").map(PathBuf::from).unwrap_or_else(|_| state.join("status.jsonl"));
    let mut samples = std::fs::File::create(&out).unwrap();
    let mut over = Vec::new();
    let mut long_report_at = None;
    let end = Instant::now() + Duration::from_secs(3000);
    while Instant::now() < end && slots.reports.len() < JOBS + 1 {
        slots.tick();
        let s = slots.status();
        writeln!(samples, "{}", serde_json::to_string(&s).unwrap()).unwrap();
        for k in &s.kinds {
            if k.busy > k.slots {
                over.push(format!("{}: busy {} over slots {}", k.name, k.busy, k.slots));
            }
        }
        // C3 after C1: the long job goes to the broker when each job of C1
        // has its report.
        if !release.load(Ordering::SeqCst) && slots.reports.len() >= JOBS {
            broker.queue(Job::scripts(LONG, &["sleep 900"]));
            release.store(true, Ordering::SeqCst);
        }
        if long_report_at.is_none() && slots.reports.iter().any(|r| r.job_id.as_str() == LONG) {
            long_report_at = Some(Instant::now());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    // C3: after the cancel, the host is idle and not degraded.
    for _ in 0..10 {
        slots.tick();
        std::thread::sleep(Duration::from_millis(500));
    }
    let after = slots.status();
    writeln!(samples, "{}", serde_json::to_string(&after).unwrap()).unwrap();
    let reports: Vec<_> = slots.reports.iter().cloned().collect();
    println!("{reports:#?}\nthe broker saw: {:?}\nstatus samples: {}", broker.seen(), out.display());
    let _ = std::fs::remove_dir_all(state.join(&kind_name));

    // C1: 20 reports of success, 20 units, each destroyed with no residue.
    let c1: Vec<_> = reports.iter().filter(|r| r.job_id.as_str().starts_with("c1-")).collect();
    assert_eq!(c1.len(), JOBS, "{reports:#?}");
    let mut units = BTreeSet::new();
    for r in &c1 {
        assert_eq!(r.exit_reason, ExitReason::Success, "C1 or C5: {r:#?}\n{}", broker.logs());
        let u = r.unit.as_ref().expect("the unit of a job");
        assert_eq!(u.unit_kind, kind_name);
        let left = |x: &Residual| matches!(x, Residual::Count(n) if *n > 0);
        assert!(!left(&u.residual_processes) && !left(&u.residual_files), "C1: a residue: {r:#?}");
        units.insert(u.unit_id.as_str().to_string());
    }
    assert_eq!(units.len(), JOBS, "C1: each job in a fresh unit: {units:?}");
    assert_eq!(broker.logs().matches("podrunner-accept-ok").count(), JOBS, "{}", broker.logs());
    // C2.
    assert!(over.is_empty(), "C2: {over:?}");
    // C3.
    let long = reports.iter().find(|r| r.job_id.as_str() == LONG).expect("C3: a report of the cancelled job");
    assert_eq!(long.exit_reason, ExitReason::Cancelled, "C3: {long:#?}");
    let at = cancelled.lock().unwrap().expect("C3: the source sent the cancel");
    let took = long_report_at.unwrap().duration_since(at);
    println!("C3: the cancel to the report of the destroyed unit: {took:?}");
    assert!(took <= CANCEL_BOUND, "C3: {took:?} over {CANCEL_BOUND:?}");
    assert!(after.kinds.iter().all(|k| k.busy == 0), "C3: {after:?}");
    assert_eq!(after.degraded, None, "C3: {after:?}");
}
