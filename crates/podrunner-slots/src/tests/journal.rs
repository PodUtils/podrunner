//! The journal around the claim (GitHub #55): the `claimed` line is on the
//! disk before the source sees the claim, a replay of it releases and
//! reports the job, and a journal that cannot be written claims nothing.

use std::sync::{Arc, Mutex};

use podrunner_schema::{Boundary, EgressHold, ExitReason, JobId, Principal};
use podrunner_source::{Event, Hello, JobSource, SourceError};
use serde_json::{Map, Value, json};

use super::{Bed, KindSpec, TempDir, capability, config, offer, sh};
use crate::state::State;

/// A source that looks at the journal when it gets a claim.
struct Probe {
    inner: Box<dyn JobSource>,
    journal: std::path::PathBuf,
    /// For each claim: whether a `claimed` line of the job was on the disk.
    seen: Arc<Mutex<Vec<bool>>>,
}

impl JobSource for Probe {
    fn hello(&mut self, c: &str, v: &str) -> Result<Hello, SourceError> {
        self.inner.hello(c, v)
    }
    fn next(&mut self, n: u32, w: std::time::Duration) -> Result<Vec<Event>, SourceError> {
        self.inner.next(n, w)
    }
    fn ack(&mut self, j: &JobId) -> Result<(), SourceError> {
        self.inner.ack(j)
    }
    fn claim(&mut self, j: &JobId, r: &str) -> Result<(), SourceError> {
        let text = std::fs::read_to_string(&self.journal).unwrap_or_default();
        let on_disk = text.lines().any(|l| l.contains(&format!("\"job_id\":\"{j}\"")) && l.contains("\"claimed\""));
        self.seen.lock().unwrap().push(on_disk);
        self.inner.claim(j, r)
    }
    fn mint(&mut self, j: &JobId, r: &str) -> Result<podrunner_schema::Credential, SourceError> {
        self.inner.mint(j, r)
    }
    fn report(&mut self, j: &JobId, r: &Map<String, Value>) -> Result<(), SourceError> {
        self.inner.report(j, r)
    }
    fn audit(&mut self, l: &Map<String, Value>) -> Result<(), SourceError> {
        self.inner.audit(l)
    }
    fn withdraw(&mut self, j: &JobId, r: &str) -> Result<(), SourceError> {
        self.inner.withdraw(j, r)
    }
    fn release(&mut self, j: &JobId) -> Result<(), SourceError> {
        self.inner.release(j)
    }
}

fn kind() -> KindSpec {
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    KindSpec { name: "fake", slots: 1, capability: cap, command: sh("exit 0") }
}

fn probed(bed: &mut Bed) -> Arc<Mutex<Vec<bool>>> {
    let seen: Arc<Mutex<Vec<bool>>> = Arc::default();
    let empty = podrunner_fake::parse_script("").unwrap();
    let placeholder = podrunner_fake::FakeSource::new("a", Arc::new(bed.clock.clone()), empty);
    let inner = std::mem::replace(&mut bed.slots.sources[0].source, Box::new(placeholder));
    let journal = bed.dir.0.join("state/journal.jsonl");
    bed.slots.sources[0].source = Box::new(Probe { inner, journal, seen: seen.clone() });
    seen
}

#[test]
fn the_claimed_line_is_on_the_disk_before_the_claim() {
    let mut bed = Bed::new(config(""), vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])], vec![kind()]);
    let seen = probed(&mut bed);
    bed.until(20, |s| s.jobs().count() == 1 && Bed::all_final(s));
    assert_eq!(*seen.lock().unwrap(), [true]);
}

#[test]
fn a_journal_that_cannot_be_written_claims_nothing() {
    let mut bed = Bed::new(config(""), vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])], vec![kind()]);
    let seen = probed(&mut bed);
    bed.slots.journal.break_for_test();
    bed.until(5, |s| s.degraded().is_some());
    bed.tick();
    assert!(seen.lock().unwrap().is_empty(), "the source saw a claim");
    assert_eq!(bed.slots.degraded(), Some("the journal cannot be written"));
    assert_ne!(bed.state("j-1"), Some(State::Claimed));
}

#[test]
fn a_replayed_claimed_line_is_released_and_reported_lost() {
    for (last, reported) in [("claimed", true), ("matched", false)] {
        let dir = TempDir::new();
        std::fs::create_dir_all(dir.0.join("state")).unwrap();
        let lines = [
            json!({ "t_ms": 0, "source": "a", "job_id": "j-1", "state": "offered" }),
            json!({ "t_ms": 0, "source": "a", "job_id": "j-1", "state": last }),
        ];
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(dir.0.join("state/journal.jsonl"), text).unwrap();
        let bed = Bed::in_dir(config(""), vec![("a", "trusted", None, vec![])], vec![kind()], dir, Default::default());
        let lost = bed.slots.reports.iter().any(|r| r.exit_reason == ExitReason::Lost);
        // A `matched` line: the claim never went out, so no release can
        // free a claim of another runner.
        assert_eq!(lost, reported, "{last}");
        let journal = std::fs::read_to_string(bed.dir.0.join("state/journal.jsonl")).unwrap();
        assert!(journal.contains("\"lost\""), "{last}: {journal}");
    }
}
