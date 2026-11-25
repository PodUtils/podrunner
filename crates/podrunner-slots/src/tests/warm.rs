//! T-487: the warm pool of slots on the fake kind, with a manual clock.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use podrunner_schema::{Boundary, EgressHold, JobId, Principal, UnitId};
use podrunner_source::{Event, JobSource, SourceError};
use serde_json::{Map, Value, json};

use super::*;
use crate::EgressPort;
use crate::config::WarmCount;

pub(super) fn confined() -> podrunner_schema::Capability {
    capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory)
}

/// A bed with one fake kind of `slots` and `warm` warm units, and the
/// script lines of one trusted source.
pub(super) fn warm_bed(slots: u32, warm: u32, lines: Vec<String>, extra: &str) -> Bed {
    let mut bed = Bed::new(
        config(extra),
        vec![("a", "trusted", None, lines)],
        vec![KindSpec { name: "fake", slots, capability: confined(), command: sh("sleep 300") }],
    );
    bed.slots.kinds[0].cfg.warm = Some(WarmCount::Count(warm));
    bed
}

pub(super) fn ready(s: &Slots) -> usize {
    s.warm.units.values().filter(|w| w.ready_ms.is_some()).count()
}

pub(super) fn warm_ids(s: &Slots) -> Vec<UnitId> {
    s.warm.units.keys().cloned().collect()
}

pub(super) fn audit_details(s: &Slots, kind: crate::audit::Kind) -> Vec<String> {
    s.audit_lines().iter().filter(|e| e.audit == kind).filter_map(|e| e.detail.clone()).collect()
}

#[test]
fn a_pool_fills_to_its_size_and_offers_the_full_capacity() {
    let mut bed = warm_bed(3, 2, vec![], "");
    bed.until(30, |s| ready(s) == 2);
    for _ in 0..3 {
        bed.tick();
    }
    assert_eq!(bed.slots.warm_units(), 2, "no unit past the size of the row");
    assert_eq!(bed.slots.capacity(0), 3, "the warm units do not lower the capacity of a source");
    let starts = audit_details(&bed.slots, crate::audit::Kind::Warm);
    assert!(
        starts.iter().filter(|d| d.starts_with("start kind=fake row=* payload=sha256:")).count() == 2,
        "{starts:?}"
    );
    // The journal has each warm unit under the source `@warm`.
    let journal = std::fs::read_to_string(bed.dir.0.join("state/journal.jsonl")).unwrap();
    assert_eq!(journal.matches("\"source\":\"@warm\"").count(), 4, "matched and started of each: {journal}");
}

#[test]
fn a_job_takes_a_ready_warm_unit_and_the_pool_fills_again() {
    let mut bed = warm_bed(2, 1, vec![offer(5, "j-1", json!({}))], "");
    bed.until(4, |s| ready(s) == 1);
    let first = warm_ids(&bed.slots)[0].clone();
    bed.until(30, |s| s.jobs().any(|j| j.state == State::Started));
    let j = bed.slots.jobs().next().unwrap().clone();
    assert_eq!((j.unit.as_ref(), j.warm), (Some(&first), true), "the job took the warm unit");
    let take = audit_details(&bed.slots, crate::audit::Kind::WarmTake);
    assert!(take[0].starts_with(&format!("unit={first} kind=fake waited_s=")), "{take:?}");
    // The claim ran the command of the kind in the warm unit.
    let o = bed.slots.kinds[0].driver.observe(&first).unwrap();
    assert!(o.runner_root.unwrap().join(".credentials").exists(), "the credential came with the claim");
    bed.until(30, |s| ready(s) == 1);
    assert_ne!(warm_ids(&bed.slots), [first], "a new warm unit");
}

/// The egress port of the tests: what it held and opened, in order.
#[derive(Clone, Default)]
pub(super) struct Seen(pub Arc<Mutex<Vec<String>>>);

impl EgressPort for Seen {
    fn open(&self, unit: &str, _policy: &str, _socket: &std::path::Path) -> Result<usize, String> {
        self.0.lock().unwrap().push(format!("open {unit}"));
        Ok(0)
    }
    fn hold(&self, unit: &str, _socket: &std::path::Path) -> Result<(), String> {
        self.0.lock().unwrap().push(format!("hold {unit}"));
        Ok(())
    }
    fn close(&self, unit: &str) {
        self.0.lock().unwrap().push(format!("close {unit}"));
    }
    fn alive(&self) -> bool {
        true
    }
}

#[test]
fn a_warm_unit_gets_no_credential_and_no_way_out_before_its_claim() {
    let seen = Seen::default();
    let bed = warm_bed(2, 1, vec![offer(8, "j-1", json!({}))], "");
    let mut bed = Bed { slots: bed.slots.with_egress(Box::new(seen.clone())), ..bed };
    bed.until(6, |s| ready(s) == 1);
    let u = warm_ids(&bed.slots)[0].clone();
    let root = bed.slots.kinds[0].driver.observe(&u).unwrap().runner_root.unwrap();
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0, "an empty runner root before the claim");
    assert_eq!(*seen.0.lock().unwrap(), [format!("hold {u}")], "held, not opened");
    bed.until(30, |s| s.jobs().any(|j| j.state == State::Started));
    let log = seen.0.lock().unwrap().clone();
    assert_eq!(&log[..2], [format!("hold {u}"), format!("open {u}")], "the open of the job comes after: {log:?}");
    assert!(root.join(".runner").exists(), "the credential after the claim");
}

#[test]
fn two_offers_in_one_tick_take_two_warm_units() {
    let mut bed = warm_bed(3, 2, vec![offer(6, "j-1", json!({})), offer(6, "j-2", json!({}))], "");
    bed.until(5, |s| ready(s) == 2);
    let warm = warm_ids(&bed.slots);
    bed.until(30, |s| s.jobs().filter(|j| j.state == State::Started).count() == 2);
    let mut units: Vec<UnitId> = bed.slots.jobs().filter_map(|j| j.unit.clone()).collect();
    units.sort();
    assert_eq!(units, warm, "each job took its own warm unit");
    assert!(bed.slots.jobs().all(|j| j.warm));
}

/// A source whose claim fails (another runner has each job) while its
/// first flag is set, and whose mint waits (`credential-unavailable`)
/// while its second flag is set.
pub(super) struct Gate(pub Box<dyn JobSource>, pub Arc<AtomicBool>, pub Arc<AtomicBool>);

impl Gate {
    /// Put a gate over the source of `bed`: its two flags.
    pub(super) fn over(bed: &mut Bed) -> (Arc<AtomicBool>, Arc<AtomicBool>) {
        let (claim, mint) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        let inner = std::mem::replace(&mut bed.slots.sources[0].source, Box::new(crate::sources::Stopped));
        bed.slots.sources[0].source = Box::new(Gate(inner, claim.clone(), mint.clone()));
        (claim, mint)
    }
}

impl JobSource for Gate {
    fn hello(&mut self, c: &str, v: &str) -> Result<podrunner_source::Hello, SourceError> {
        self.0.hello(c, v)
    }
    fn next(&mut self, n: u32, w: std::time::Duration) -> Result<Vec<Event>, SourceError> {
        self.0.next(n, w)
    }
    fn ack(&mut self, j: &JobId) -> Result<(), SourceError> {
        self.0.ack(j)
    }
    fn claim(&mut self, j: &JobId, r: &str) -> Result<(), SourceError> {
        if self.1.load(Ordering::SeqCst) {
            return Err(SourceError::new(podrunner_source::SourceCode::AlreadyClaimed, format!("{j}")));
        }
        self.0.claim(j, r)
    }
    fn mint(&mut self, j: &JobId, r: &str) -> Result<podrunner_schema::Credential, SourceError> {
        if self.2.load(Ordering::SeqCst) {
            return Err(SourceError::new(podrunner_source::SourceCode::CredentialUnavailable, format!("{j}")));
        }
        self.0.mint(j, r)
    }
    fn report(&mut self, j: &JobId, r: &Map<String, Value>) -> Result<(), SourceError> {
        self.0.report(j, r)
    }
    fn audit(&mut self, l: &Map<String, Value>) -> Result<(), SourceError> {
        self.0.audit(l)
    }
    fn withdraw(&mut self, j: &JobId, r: &str) -> Result<(), SourceError> {
        self.0.withdraw(j, r)
    }
    fn release(&mut self, j: &JobId) -> Result<(), SourceError> {
        self.0.release(j)
    }
}

/// P2-2: the source had the runner name of the warm unit: the unit is
/// destroyed, not given back to the pool.
#[test]
fn a_failed_source_claim_destroys_the_warm_unit() {
    let mut bed = warm_bed(2, 1, vec![offer(5, "j-1", json!({}))], "");
    bed.until(4, |s| ready(s) == 1);
    let u = warm_ids(&bed.slots)[0].clone();
    Gate::over(&mut bed).0.store(true, Ordering::SeqCst);
    bed.until(10, |s| s.jobs().any(|j| j.state == State::Released));
    assert!(!warm_ids(&bed.slots).contains(&u), "the unit left the pool");
    assert!(audit_details(&bed.slots, crate::audit::Kind::Warm).contains(&"end reason=job-ended".to_string()));
    assert!(bed.slots.kinds[0].driver.observe(&u).is_err(), "destroyed");
}

/// P2-3: a failure of the claim of the driver destroys the warm unit and
/// fails the job as a failed start does.
#[test]
fn a_failed_claim_destroys_the_warm_unit_and_fails_the_job_as_a_start() {
    let mut bed = warm_bed(2, 1, vec![offer(5, "j-1", json!({}))], "");
    bed.until(4, |s| ready(s) == 1);
    let u = warm_ids(&bed.slots)[0].clone();
    // The claim cannot write the credential: its runner root is gone.
    let root = bed.slots.kinds[0].driver.observe(&u).unwrap().runner_root.unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    bed.until(10, |s| s.jobs().count() == 1 && Bed::all_final(s));
    let j = bed.slots.jobs().next().unwrap();
    assert_eq!((j.state, j.refusal), (State::Failed, Some(podrunner_schema::RefusalCode::UnitStartFailed)));
    assert!(bed.slots.kinds[0].driver.observe(&u).is_err(), "destroyed");
    assert!(bed.slots.degraded().is_none());
}

/// An offer whose limits the kind would apply below the limits of the warm
/// unit starts cold, with a `warm-miss` line that says why.
#[test]
fn an_offer_with_lower_limits_starts_cold_with_a_warm_miss_line() {
    let low = json!({ "limits": { "pids": 64 } });
    let mut bed = warm_bed(3, 1, vec![offer(5, "j-1", low)], "");
    let k = podrunner_unit::limits::Limits { pids: Some(1000), ..Default::default() };
    let driver = podrunner_fake::FakeUnit::new(&bed.dir.0.join("lim"), confined(), "linux/amd64").with_limits(k);
    bed.slots.kinds[0].driver = Arc::new(driver);
    bed.until(4, |s| ready(s) == 1);
    let u = warm_ids(&bed.slots)[0].clone();
    bed.until(30, |s| s.jobs().any(|j| j.state == State::Started));
    let j = bed.slots.jobs().next().unwrap();
    assert!(!j.warm && j.unit.as_ref() != Some(&u), "a cold unit");
    assert_eq!(audit_details(&bed.slots, crate::audit::Kind::WarmMiss), ["reason=limits"]);
    assert_eq!(warm_ids(&bed.slots), [u], "the warm unit stays");
}

/// The slots are full of warm units of another payload: the oldest ends,
/// then the job starts cold.
#[test]
fn an_offer_of_another_payload_evicts_the_oldest_warm_unit_when_the_slots_are_full() {
    let gpu = "[[pin.payload]]\narch = \"x86_64\"\nreference = \"gpu@sha256:2222222222222222222222222222222222222222222222222222222222222222\"\nbuilt_at_s = 1800000000\nlabels = [\"gpu\"]\n";
    let mut cfg = config("");
    cfg.pin.payload.extend(Config::parse(&format!("host = \"h\"\n[pin]\n{gpu}")).unwrap().pin.payload);
    let lines = vec![offer(8, "j-1", json!({ "labels": ["gpu"] }))];
    let mut bed = Bed::new(
        cfg,
        vec![("a", "trusted", None, lines)],
        vec![KindSpec { name: "fake", slots: 2, capability: confined(), command: sh("sleep 300") }],
    );
    bed.slots.kinds[0].cfg.warm = Some(WarmCount::Count(2));
    bed.until(6, |s| ready(s) == 2);
    let oldest = bed.slots.warm.units.iter().min_by_key(|(_, w)| w.made_ms).map(|(u, _)| u.clone()).unwrap();
    bed.until(30, |s| s.jobs().any(|j| j.state == State::Started));
    assert_eq!(audit_details(&bed.slots, crate::audit::Kind::WarmMiss), ["reason=payload"]);
    assert!(!warm_ids(&bed.slots).contains(&oldest), "the oldest warm unit made way");
    assert!(audit_details(&bed.slots, crate::audit::Kind::Warm).contains(&"end reason=evict".to_string()));
    let views = bed.slots.kind_views();
    assert!(views[0].running + bed.slots.warm_free(0) <= 2, "never over the slots");
}

#[test]
fn max_units_counts_warm_units() {
    let mut bed = warm_bed(3, 3, vec![], "max_units = 2");
    bed.until(10, |s| ready(s) == 2);
    for _ in 0..3 {
        bed.tick();
    }
    assert_eq!(bed.slots.warm_units(), 2, "max_units holds the warm units too");
}

/// `stop` drains: each free warm unit ends on the next tick, and none is
/// made (`serve` exits only with none left).
#[test]
fn drain_ends_each_warm_unit_and_makes_none() {
    let mut bed = warm_bed(2, 2, vec![], "");
    bed.until(6, |s| ready(s) == 2);
    bed.slots.drain();
    bed.tick();
    assert_eq!(bed.slots.warm_units(), 0);
    for _ in 0..3 {
        bed.tick();
    }
    assert_eq!(bed.slots.warm_units(), 0, "no refill while draining");
    let ends = audit_details(&bed.slots, crate::audit::Kind::Warm);
    assert_eq!(ends.iter().filter(|d| *d == "end reason=drain").count(), 2, "{ends:?}");
}

/// A restart destroys each warm unit of the journal, and reports nothing:
/// no source had it.
#[test]
fn a_restart_destroys_the_warm_units_of_the_journal_and_reports_nothing() {
    let mut bed = warm_bed(2, 1, vec![], "");
    bed.until(4, |s| ready(s) == 1);
    let u = warm_ids(&bed.slots)[0].clone();
    let root = bed.slots.kinds[0].driver.observe(&u).unwrap().unit_root.unwrap();
    let Bed { slots, clock, dir } = bed;
    let cfg = slots.cfg.clone();
    drop(slots);
    assert!(root.exists(), "the unit is there before the restart");
    let kind = KindSpec { name: "fake", slots: 2, capability: confined(), command: sh("sleep 300") };
    let again = Bed::in_dir(cfg, vec![("a", "trusted", None, vec![])], vec![kind], dir, clock);
    assert!(!root.exists(), "the restart destroyed the warm unit");
    assert!(again.slots.reports.is_empty(), "no report of a warm unit");
    assert_eq!((again.slots.warm_units(), again.slots.degraded()), (0, None));
    let journal = std::fs::read_to_string(again.dir.0.join("state/journal.jsonl")).unwrap();
    assert!(journal.lines().any(|l| l.contains("@warm") && l.contains("\"lost\"")), "{journal}");
}

/// Placement runs first: a kind below the floor of the job never gives it
/// its warm unit; among the kinds that meet the job, one with a warm unit
/// of its payload comes first.
#[test]
fn the_tier_rules_are_the_same_with_a_warm_pool() {
    let low = capability(Boundary::Redirect, Principal::Distinct, EgressHold::Advisory);
    let kinds = |a: (&'static str, podrunner_schema::Capability), b: (&'static str, podrunner_schema::Capability)| {
        vec![
            KindSpec { name: a.0, slots: 2, capability: a.1, command: sh("sleep 300") },
            KindSpec { name: b.0, slots: 2, capability: b.1, command: sh("sleep 300") },
        ]
    };
    // The warm unit is on a kind below the floor of `trusted`.
    let lines = vec![offer(5, "j-1", json!({}))];
    let mut bed = Bed::new(config(""), vec![("a", "trusted", None, lines)], kinds(("low", low), ("high", confined())));
    bed.slots.kinds[0].cfg.warm = Some(WarmCount::Count(1));
    bed.until(4, |s| ready(s) == 1);
    bed.until(30, |s| s.jobs().any(|j| j.state == State::Started));
    let j = bed.slots.jobs().next().unwrap();
    assert_eq!((j.placement.as_ref().unwrap().kind, j.warm), (1, false), "cold, on the kind at the floor");
    assert_eq!(bed.slots.warm_units(), 1, "the warm unit below the floor stays");
    // Two kinds meet the job: the second has the warm unit, and wins.
    let lines = vec![offer(5, "j-1", json!({}))];
    let mut bed =
        Bed::new(config(""), vec![("a", "trusted", None, lines)], kinds(("a", confined()), ("b", confined())));
    bed.slots.kinds[1].cfg.warm = Some(WarmCount::Count(1));
    bed.until(4, |s| ready(s) == 1);
    bed.until(30, |s| s.jobs().any(|j| j.state == State::Started));
    let j = bed.slots.jobs().next().unwrap();
    assert_eq!((j.placement.as_ref().unwrap().kind, j.warm), (1, true), "the kind with the warm unit");
}
