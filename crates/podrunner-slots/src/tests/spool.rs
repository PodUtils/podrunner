//! T-069: a report that the source refuses goes to the spool, with an
//! audit line, and reaches the source when it answers again, also after a
//! restart.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use podrunner_schema::{Boundary, EgressHold, JobId, Principal};
use podrunner_source::{Event, Hello, JobSource, SourceCode, SourceError};
use serde_json::{Map, Value, json};

use super::{Bed, KindSpec, capability, config, offer, sh};

/// A source that refuses each report while `refuse` is set, and counts the
/// reports it takes.
struct Refusing {
    inner: Box<dyn JobSource>,
    refuse: Arc<AtomicBool>,
    taken: Arc<Mutex<Vec<String>>>,
}

impl JobSource for Refusing {
    fn hello(&mut self, c: &str, v: &str) -> Result<Hello, SourceError> {
        self.inner.hello(c, v)
    }
    fn next(&mut self, n: u32, w: Duration) -> Result<Vec<Event>, SourceError> {
        self.inner.next(n, w)
    }
    fn ack(&mut self, j: &JobId) -> Result<(), SourceError> {
        self.inner.ack(j)
    }
    fn claim(&mut self, j: &JobId, r: &str) -> Result<(), SourceError> {
        self.inner.claim(j, r)
    }
    fn mint(&mut self, j: &JobId, r: &str) -> Result<podrunner_schema::Credential, SourceError> {
        self.inner.mint(j, r)
    }
    fn report(&mut self, j: &JobId, _: &Map<String, Value>) -> Result<(), SourceError> {
        if self.refuse.load(Ordering::SeqCst) {
            return Err(SourceError::new(SourceCode::Unavailable, "the sidecar restarts"));
        }
        self.taken.lock().unwrap().push(j.to_string());
        Ok(())
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

fn refusing(bed: &mut Bed, refuse: &Arc<AtomicBool>) -> Arc<Mutex<Vec<String>>> {
    let taken: Arc<Mutex<Vec<String>>> = Arc::default();
    let empty = podrunner_fake::parse_script("").unwrap();
    let placeholder = podrunner_fake::FakeSource::new("a", Arc::new(bed.clock.clone()), empty);
    let inner = std::mem::replace(&mut bed.slots.sources[0].source, Box::new(placeholder));
    bed.slots.sources[0].source = Box::new(Refusing { inner, refuse: refuse.clone(), taken: taken.clone() });
    taken
}

#[test]
fn report_spool_keeps_a_refused_report_until_the_source_takes_it() {
    let refuse = Arc::new(AtomicBool::new(true));
    let mut bed = Bed::new(config(""), vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])], vec![kind()]);
    let taken = refusing(&mut bed, &refuse);
    bed.until(20, |s| s.jobs().count() == 1 && super::Bed::all_final(s));
    assert_eq!(bed.slots.spooled(), 1);
    assert_eq!(bed.slots.status().spooled_reports, 1);
    let audit = std::fs::read_to_string(bed.dir.0.join("state/audit.jsonl")).unwrap();
    assert!(audit.contains("\"report-spooled\""), "{audit}");
    // The source answers again: the next send takes the report.
    refuse.store(false, Ordering::SeqCst);
    bed.clock.advance(Duration::from_secs(31));
    bed.tick();
    assert_eq!(*taken.lock().unwrap(), ["j-1"]);
    assert_eq!(bed.slots.spooled(), 0);
}

#[test]
fn report_spool_survives_a_restart() {
    let refuse = Arc::new(AtomicBool::new(true));
    let mut bed = Bed::new(config(""), vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])], vec![kind()]);
    refusing(&mut bed, &refuse);
    bed.until(20, |s| s.jobs().count() == 1 && super::Bed::all_final(s));
    assert_eq!(bed.slots.spooled(), 1);
    let Bed { slots, clock, dir } = bed;
    drop(slots);
    let mut bed = Bed::in_dir(config(""), vec![("a", "trusted", None, vec![])], vec![kind()], dir, clock);
    refuse.store(false, Ordering::SeqCst);
    let taken = refusing(&mut bed, &refuse);
    // The first tick after a start sends the spool.
    bed.tick();
    assert_eq!(*taken.lock().unwrap(), ["j-1"]);
    assert_eq!(bed.slots.spooled(), 0);
}
