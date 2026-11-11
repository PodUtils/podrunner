//! The conformance suite of the source seam: the tests 1 to 12 of
//! `docs/contracts/source-driver.md`. Each test takes a fresh source from
//! the fixture. The script of a fresh source holds two offers or more, which
//! the first `next` can give.

use std::fmt;
use std::time::Duration;

use podrunner_schema::JobId;
use serde_json::Map;

use crate::{Event, JobSource, SourceCode, SourceError, serve_line, wire};

/// What a test needs from the source under test.
pub trait Fixture {
    /// A fresh source, with two offers or more for its first `next`.
    fn fresh(&self) -> Box<dyn JobSource>;

    /// Send one line of the wire to a fresh source, and give its reply line.
    /// A source in another process replaces this.
    fn exchange(&self, line: &[u8]) -> String {
        serve_line(self.fresh().as_mut(), line)
    }
}

/// The result of one test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub number: u32,
    pub name: &'static str,
    /// `None` is a pass; else the reason of the failure.
    pub failure: Option<String>,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.failure {
            None => write!(f, "source {:2} {}: pass", self.number, self.name),
            Some(why) => write!(f, "source {:2} {}: FAIL: {why}", self.number, self.name),
        }
    }
}

type Run = fn(&dyn Fixture) -> Result<(), String>;

const CASES: [(u32, &str, Run); 12] = [
    (1, "hello-names-the-source", hello_names_the_source),
    (2, "no-capacity-gives-no-offer", no_capacity_gives_no_offer),
    (3, "next-gives-at-most-capacity-offers", next_gives_at_most_capacity_offers),
    (4, "ack-of-an-offer", ack_of_an_offer),
    (5, "a-job-is-claimed-once", a_job_is_claimed_once),
    (6, "mint-needs-a-claim", mint_needs_a_claim),
    (7, "report-of-a-claimed-job", report_of_a_claimed_job),
    (8, "audit-is-accepted", audit_is_accepted),
    (9, "withdraw-ends-an-offer", withdraw_ends_an_offer),
    (10, "release-ends-a-claim", release_ends_a_claim),
    (11, "an-unknown-op-is-bad-request", an_unknown_op_is_bad_request),
    (12, "the-wire-refuses-a-long-line-a-path-and-a-schema", the_wire_refuses_bad_lines),
];

/// The number and the name of each test, in order.
pub fn tests() -> Vec<(u32, &'static str)> {
    CASES.iter().map(|(n, name, _)| (*n, *name)).collect()
}

/// Run the tests with the numbers in `only` (all when empty), in order.
pub fn run(fixture: &dyn Fixture, only: &[u32]) -> Vec<Report> {
    CASES
        .iter()
        .filter(|(n, _, _)| only.is_empty() || only.contains(n))
        .map(|(number, name, run)| Report { number: *number, name, failure: run(fixture).err() })
        .collect()
}

/// The numbers of the tests that did not pass.
pub fn not_passed(results: &[Report]) -> Vec<u32> {
    results.iter().filter(|r| r.failure.is_some()).map(|r| r.number).collect()
}

/// The time of a long poll in the tests.
const WAIT: Duration = Duration::from_secs(5);

fn err(what: &str) -> impl Fn(SourceError) -> String + '_ {
    move |e| format!("{what}: {e}")
}

fn refused<T: fmt::Debug>(what: &str, got: Result<T, SourceError>, code: SourceCode) -> Result<(), String> {
    match got {
        Err(e) if e.code == code => Ok(()),
        Err(e) => Err(format!("{what}: refused with {e}, not {}", code.as_str())),
        Ok(v) => Err(format!("{what}: accepted ({v:?}), not refused with {}", code.as_str())),
    }
}

/// A fresh source and the job id of its first offer.
fn offered(fx: &dyn Fixture) -> Result<(Box<dyn JobSource>, JobId), String> {
    let mut s = fx.fresh();
    let events = s.next(1, WAIT).map_err(err("next"))?;
    let job = events.iter().find_map(|e| match e {
        Event::Offer { offer } => Some(offer.job_id.clone()),
        _ => None,
    });
    Ok((s, job.ok_or("the first next gives no offer")?))
}

/// A fresh source with its first offer acked and claimed.
fn claimed(fx: &dyn Fixture) -> Result<(Box<dyn JobSource>, JobId), String> {
    let (mut s, job) = offered(fx)?;
    s.ack(&job).map_err(err("ack"))?;
    s.claim(&job, "runner-u-1").map_err(err("claim"))?;
    Ok((s, job))
}

fn hello_names_the_source(fx: &dyn Fixture) -> Result<(), String> {
    let h = fx.fresh().hello("podrunner", "0").map_err(err("hello"))?;
    if h.source.is_empty() { Err("hello gives no source name".into()) } else { Ok(()) }
}

fn no_capacity_gives_no_offer(fx: &dyn Fixture) -> Result<(), String> {
    let events = fx.fresh().next(0, Duration::ZERO).map_err(err("next"))?;
    match events.iter().find(|e| matches!(e, Event::Offer { .. })) {
        Some(e) => Err(format!("next with capacity 0 gives an offer of {}", e.job_id())),
        None => Ok(()),
    }
}

fn next_gives_at_most_capacity_offers(fx: &dyn Fixture) -> Result<(), String> {
    let events = fx.fresh().next(1, WAIT).map_err(err("next"))?;
    match events.iter().filter(|e| matches!(e, Event::Offer { .. })).count() {
        1 => Ok(()),
        n => Err(format!("next with capacity 1 gives {n} offers, not 1 (the script holds two or more)")),
    }
}

fn ack_of_an_offer(fx: &dyn Fixture) -> Result<(), String> {
    let (mut s, job) = offered(fx)?;
    s.ack(&job).map_err(err("ack"))?;
    let never = JobId::new("never-offered-0").expect("a job id");
    refused("ack of a job that was not offered", s.ack(&never), SourceCode::UnknownJob)
}

fn a_job_is_claimed_once(fx: &dyn Fixture) -> Result<(), String> {
    let (mut s, job) = claimed(fx)?;
    refused("a second claim", s.claim(&job, "runner-u-2"), SourceCode::AlreadyClaimed)
}

fn mint_needs_a_claim(fx: &dyn Fixture) -> Result<(), String> {
    let (mut s, job) = offered(fx)?;
    s.ack(&job).map_err(err("ack"))?;
    refused("mint before claim", s.mint(&job, "runner-u-1"), SourceCode::NotClaimed)?;
    s.claim(&job, "runner-u-1").map_err(err("claim"))?;
    let c = s.mint(&job, "runner-u-1").map_err(err("mint"))?;
    if c.files.is_empty() || !c.files.iter().all(|f| f.name_is_plain()) {
        return Err("mint gives no file, or a file name that is not plain".into());
    }
    Ok(())
}

fn report_of_a_claimed_job(fx: &dyn Fixture) -> Result<(), String> {
    let (mut s, job) = claimed(fx)?;
    let mut r = Map::new();
    r.insert("exit_reason".into(), "success".into());
    s.report(&job, &r).map_err(err("report"))
}

fn audit_is_accepted(fx: &dyn Fixture) -> Result<(), String> {
    let mut line = Map::new();
    line.insert("kind".into(), "offer".into());
    fx.fresh().audit(&line).map_err(err("audit"))
}

fn withdraw_ends_an_offer(fx: &dyn Fixture) -> Result<(), String> {
    let (mut s, job) = offered(fx)?;
    s.ack(&job).map_err(err("ack"))?;
    s.withdraw(&job, "wait-deadline").map_err(err("withdraw"))?;
    refused("claim after withdraw", s.claim(&job, "runner-u-1"), SourceCode::UnknownJob)
}

fn release_ends_a_claim(fx: &dyn Fixture) -> Result<(), String> {
    let (mut s, job) = claimed(fx)?;
    s.release(&job).map_err(err("release"))?;
    refused("claim after release", s.claim(&job, "runner-u-2"), SourceCode::UnknownJob)
}

/// The code of an error reply, or a failure.
fn error_code(fx: &dyn Fixture, line: &[u8]) -> Result<SourceCode, String> {
    match wire::parse_reply(fx.exchange(line).as_bytes()) {
        Ok(wire::Reply::Error { code, .. }) => Ok(code),
        Ok(other) => Err(format!("the reply is {other:?}, not an error")),
        Err(e) => Err(format!("the reply is not a line of the wire: {e}")),
    }
}

fn bad_request(fx: &dyn Fixture, what: &str, line: &[u8]) -> Result<(), String> {
    match error_code(fx, line)? {
        SourceCode::BadRequest => Ok(()),
        other => Err(format!("{what}: {}, not bad-request", other.as_str())),
    }
}

fn an_unknown_op_is_bad_request(fx: &dyn Fixture) -> Result<(), String> {
    bad_request(fx, "an unknown op", br#"{"schema":1,"op":"steal","job_id":"j-1"}"#)
}

fn the_wire_refuses_bad_lines(fx: &dyn Fixture) -> Result<(), String> {
    let mut long = br#"{"schema":1,"op":"audit","line":{"pad":""#.to_vec();
    long.resize(wire::LINE_MAX + 1, b'a');
    long.extend_from_slice(br#""}}"#);
    bad_request(fx, "a line over 1 MiB", &long)?;
    bad_request(fx, "a job_id of ../x", br#"{"schema":1,"op":"ack","job_id":"../x"}"#)?;
    bad_request(fx, "schema 2", br#"{"schema":2,"op":"hello","client":"c","version":"v"}"#)?;
    bad_request(fx, "no schema", br#"{"op":"hello","client":"c","version":"v"}"#)
}
