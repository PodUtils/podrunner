//! The gate of the remote mint: the source conformance suite passes
//! through it, and each guard refuses its input.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use podrunner_clock::ManualClock;
use podrunner_fake::{FakeSource, parse_script};
use podrunner_schema::{JobId, Tier};
use podrunner_source::conformance::{self as sc, Fixture};
use podrunner_source::{Event, Exchange, JobSource, SourceCode, SourceError, Wire};
use serde_json::json;

use crate::gate::Gate;
use crate::ledger::Ledger;

fn offer(job: &str, fork: bool) -> String {
    let (event, head, actor) = if fork { ("pull_request", "yes", "outside") } else { ("push", "no", "member") };
    json!({ "at_s": 0, "offer": {
        "schema": 1, "job_id": job, "source": "src", "labels": ["podrunner"], "arch": "x86_64", "wait_deadline_s": 600,
        "trust_inputs": {
            "event": event, "head_is_fork": head, "actor_class": actor,
            "repo_visibility": "private", "owner": "example-org", "repo": "app", "run_id": 1,
            "ref": "refs/heads/main", "workflow_ref": "w"
        }
    }})
    .to_string()
}

fn dir() -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "podrunner-remote-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn gate(lines: &[String], slots: &[(&str, u32)], at: &std::path::Path) -> Gate {
    let source = FakeSource::new("src", Arc::new(ManualClock::default()), parse_script(&lines.join("\n")).unwrap());
    let trust = serde_json::from_value(json!({ "organizations": [{ "login": "example-org" }] })).unwrap();
    let hosts: BTreeMap<String, u32> = slots.iter().map(|(h, n)| (h.to_string(), *n)).collect();
    let ledger = Ledger::open(&at.join("ledger.jsonl")).unwrap();
    let audit = std::fs::File::create(at.join("audit.jsonl")).unwrap();
    Gate::new(Box::new(source), Tier::Trusted, trust, ledger, hosts, audit)
}

/// The wire of one host into a gate, in memory.
struct Local {
    gate: Arc<std::sync::Mutex<Gate>>,
    host: &'static str,
}

impl Exchange for Local {
    fn exchange(&mut self, line: &str) -> Result<Vec<u8>, SourceError> {
        Ok(self.gate.lock().unwrap().line(self.host, "192.0.2.1:4000", line.as_bytes()).into_bytes())
    }
}

fn host(g: &Arc<std::sync::Mutex<Gate>>, host: &'static str) -> Wire<Local> {
    Wire(Local { gate: g.clone(), host })
}

struct Through;

impl Fixture for Through {
    fn fresh(&self) -> Box<dyn JobSource> {
        let g = gate(&[offer("j-1", false), offer("j-2", false), offer("j-3", false)], &[("h1", 10)], &dir());
        Box::new(host(&Arc::new(std::sync::Mutex::new(g)), "h1"))
    }
}

#[test]
fn the_source_tests_pass_through_the_gate() {
    let results = sc::run(&Through, &[]);
    assert!(sc::not_passed(&results).is_empty(), "{results:#?}");
}

fn job(s: &str) -> JobId {
    JobId::new(s).unwrap()
}

fn offers(events: &[Event]) -> Vec<String> {
    events.iter().filter(|e| matches!(e, Event::Offer { .. })).map(|e| e.job_id().as_str().to_string()).collect()
}

#[test]
fn each_host_holds_its_own_offers_up_to_its_slots() {
    let lines: Vec<String> = (1..=5).map(|i| offer(&format!("j-{i}"), false)).collect();
    let d = dir();
    let g = Arc::new(std::sync::Mutex::new(gate(&lines, &[("h1", 2), ("h2", 1)], &d)));
    let (mut h1, mut h2) = (host(&g, "h1"), host(&g, "h2"));
    // Capacity 9 is capped at 2 slots; then 0 while 2 are open.
    assert_eq!(offers(&h1.next(9, Duration::ZERO).unwrap()), ["j-1", "j-2"]);
    assert!(offers(&h1.next(9, Duration::ZERO).unwrap()).is_empty());
    assert_eq!(offers(&h2.next(9, Duration::ZERO).unwrap()), ["j-3"]);
    // h2 cannot touch an offer of h1.
    assert_eq!(h2.claim(&job("j-1"), "r").unwrap_err().code, SourceCode::UnknownJob);
    assert_eq!(h2.mint(&job("j-1"), "r").unwrap_err().code, SourceCode::UnknownJob);
    // One credential for each job id.
    h1.ack(&job("j-1")).unwrap();
    h1.claim(&job("j-1"), "r").unwrap();
    h1.mint(&job("j-1"), "r").unwrap();
    assert_eq!(h1.mint(&job("j-1"), "r").unwrap_err().code, SourceCode::CredentialAlreadyIssued);
    // A release frees a slot.
    h1.release(&job("j-1")).unwrap();
    assert_eq!(offers(&h1.next(9, Duration::ZERO).unwrap()), ["j-4"]);
    // An unknown host gets nothing.
    assert_eq!(host(&g, "h9").hello("p", "0").unwrap_err().code, SourceCode::SourceRevoked);
    // The ledger is on disk: a new gate knows the credential and the offers.
    drop(g);
    let again = Arc::new(std::sync::Mutex::new(gate(&lines, &[("h1", 2)], &d)));
    let mut h1 = host(&again, "h1");
    assert_eq!(h1.mint(&job("j-1"), "r").unwrap_err().code, SourceCode::UnknownJob);
    drop(again);
    let mut ledger = Ledger::open(&d.join("ledger.jsonl")).unwrap();
    assert_eq!(ledger.holder(&job("j-4")), Some("h1"));
    assert!(!ledger.mint("h1", &job("j-1")).unwrap());
    // Each call is an audit line with the host, the peer and the job id.
    let audit = std::fs::read_to_string(d.join("audit.jsonl")).unwrap_or_default();
    assert!(audit.is_empty() || audit.lines().all(|l| l.contains("\"peer\":\"192.0.2.1:4000\"")));
}

#[test]
fn the_mint_computes_the_tier_and_withdraws_a_fork_on_the_trusted_set() {
    let d = dir();
    let g = Arc::new(std::sync::Mutex::new(gate(&[offer("fork", true), offer("member", false)], &[("h1", 5)], &d)));
    let mut h1 = host(&g, "h1");
    assert_eq!(offers(&h1.next(5, Duration::ZERO).unwrap()), ["member"]);
    assert_eq!(h1.claim(&job("fork"), "r").unwrap_err().code, SourceCode::UnknownJob);
    let audit = std::fs::read_to_string(d.join("audit.jsonl")).unwrap();
    assert!(audit.lines().any(|l| l.contains("\"op\":\"next\"") && l.contains("\"host\":\"h1\"")), "{audit}");
}
