//! The wire, the two ends over a pipe, and the harness against a source
//! that refuses each request.

use std::io::BufReader;
use std::time::Duration;

use podrunner_schema::{Credential, JobId};
use serde_json::{Map, Value};

use crate::conformance::{self, Fixture};
use crate::wire::{self, LINE_MAX, OPS, Reply, Request};
use crate::{Event, Hello, JobSource, SourceCode, SourceError, WireSource, serve_line};

/// A source that refuses each request: the harness must fail each test.
struct Refuser;

fn no<T>() -> Result<T, SourceError> {
    Err(SourceError::new(SourceCode::Unavailable, "the refuser does nothing"))
}

impl JobSource for Refuser {
    fn hello(&mut self, _: &str, _: &str) -> Result<Hello, SourceError> {
        no()
    }
    fn next(&mut self, _: u32, _: Duration) -> Result<Vec<Event>, SourceError> {
        no()
    }
    fn ack(&mut self, _: &JobId) -> Result<(), SourceError> {
        no()
    }
    fn claim(&mut self, _: &JobId, _: &str) -> Result<(), SourceError> {
        no()
    }
    fn mint(&mut self, _: &JobId, _: &str) -> Result<Credential, SourceError> {
        no()
    }
    fn report(&mut self, _: &JobId, _: &Map<String, Value>) -> Result<(), SourceError> {
        no()
    }
    fn audit(&mut self, _: &Map<String, Value>) -> Result<(), SourceError> {
        no()
    }
    fn withdraw(&mut self, _: &JobId, _: &str) -> Result<(), SourceError> {
        no()
    }
    fn release(&mut self, _: &JobId) -> Result<(), SourceError> {
        no()
    }
}

struct RefuserFixture;

impl Fixture for RefuserFixture {
    fn fresh(&self) -> Box<dyn JobSource> {
        Box::new(Refuser)
    }
    // A wire that answers each line with `done`: the wire tests fail too.
    fn exchange(&self, _: &[u8]) -> String {
        wire::reply_line(&Reply::Done)
    }
}

#[test]
fn the_harness_fails_each_test_on_a_refuser() {
    let results = conformance::run(&RefuserFixture, &[]);
    assert_eq!(results.iter().map(|r| r.number).collect::<Vec<_>>(), (1..=12).collect::<Vec<_>>());
    for r in &results {
        assert!(r.failure.is_some(), "{r}");
        assert!(r.to_string().starts_with(&format!("source {:2} {}: FAIL", r.number, r.name)), "{r}");
    }
}

#[test]
fn the_wire_tests_pass_on_serve_line() {
    // serve_line refuses bad lines before the source sees them.
    struct Plain;
    impl Fixture for Plain {
        fn fresh(&self) -> Box<dyn JobSource> {
            Box::new(Refuser)
        }
    }
    let results = conformance::run(&Plain, &[11, 12]);
    assert!(results.iter().all(|r| r.failure.is_none()), "{results:?}");
}

#[test]
fn a_request_round_trips_with_its_schema() {
    let job = JobId::new("j-1").unwrap();
    let reqs = [
        Request::Hello { client: "podrunner".into(), version: "0.1.0".into() },
        Request::Next { capacity: 2, wait_s: 30 },
        Request::Ack { job_id: job.clone() },
        Request::Claim { job_id: job.clone(), runner_name: "r".into() },
        Request::Mint { job_id: job.clone(), runner_name: "r".into() },
        Request::Report { job_id: job.clone(), report: Map::new() },
        Request::Audit { line: Map::new() },
        Request::Withdraw { job_id: job.clone(), reason: "wait-deadline".into() },
        Request::Release { job_id: job },
    ];
    assert_eq!(reqs.iter().map(Request::op).collect::<Vec<_>>(), OPS);
    for r in reqs {
        let line = wire::request_line(&r);
        assert!(line.starts_with("{\"schema\":1,\"op\":") && line.ends_with('\n'), "{line}");
        assert_eq!(wire::parse_request(line.as_bytes()).unwrap(), r);
    }
}

#[test]
fn bad_lines_are_bad_requests_and_name_no_value() {
    let cases: [&[u8]; 6] = [
        b"not json",
        br#"{"schema":1,"op":"steal"}"#,
        br#"{"schema":1,"op":"ack","job_id":"../x"}"#,
        br#"{"schema":3,"op":"release","job_id":"j"}"#,
        br#"{"schema":1,"op":"ack","job_id":"j","extra":"secret-value-41"}"#,
        br#"{"schema":1,"op":"next","capacity":"secret-value-41","wait_s":1}"#,
    ];
    for line in cases {
        let e = wire::parse_request(line).unwrap_err();
        assert_eq!(e.code, SourceCode::BadRequest, "{}", String::from_utf8_lossy(line));
        assert!(!e.detail.contains("secret-value-41") && !e.detail.contains("../x"), "{}", e.detail);
    }
    let mut long = vec![b' '; LINE_MAX];
    long.extend_from_slice(br#"{"schema":1,"op":"release","job_id":"j"}"#);
    assert_eq!(wire::parse_request(&long).unwrap_err().code, SourceCode::BadRequest);
}

#[test]
fn the_contract_names_the_operations_and_the_tests() {
    let doc = include_str!("../../../docs/contracts/source-driver.md");
    let section = doc.split("\n## Operations\n").nth(1).expect("a section Operations").split("\n## ").next().unwrap();
    let ops: Vec<&str> = section
        .lines()
        .filter_map(|l| l.strip_prefix("| `"))
        .filter_map(|l| l.split_once('`').map(|(op, _)| op))
        .collect();
    assert_eq!(ops, OPS, "the table of operations of docs/contracts/source-driver.md");
    for (n, name) in conformance::tests() {
        let row = format!("| {n} | `{name}` |");
        assert!(doc.contains(&row), "docs/contracts/source-driver.md has no row {row}");
    }
}

/// A source that answers `hello`, over a pipe of two threads.
#[test]
fn wire_source_and_serve_line_speak_over_a_pipe() {
    use std::io::{BufRead, Write};
    use std::os::unix::net::UnixStream;

    struct Named;
    impl JobSource for Named {
        fn hello(&mut self, _: &str, _: &str) -> Result<Hello, SourceError> {
            Ok(Hello { source: "fake".into(), operator_controlled: true })
        }
        fn next(&mut self, _: u32, _: Duration) -> Result<Vec<Event>, SourceError> {
            no()
        }
        fn ack(&mut self, _: &JobId) -> Result<(), SourceError> {
            Ok(())
        }
        fn claim(&mut self, _: &JobId, _: &str) -> Result<(), SourceError> {
            Err(SourceError::new(SourceCode::AlreadyClaimed, "claimed"))
        }
        fn mint(&mut self, _: &JobId, _: &str) -> Result<Credential, SourceError> {
            no()
        }
        fn report(&mut self, _: &JobId, _: &Map<String, Value>) -> Result<(), SourceError> {
            no()
        }
        fn audit(&mut self, _: &Map<String, Value>) -> Result<(), SourceError> {
            no()
        }
        fn withdraw(&mut self, _: &JobId, _: &str) -> Result<(), SourceError> {
            no()
        }
        fn release(&mut self, _: &JobId) -> Result<(), SourceError> {
            no()
        }
    }

    let (ours, theirs) = UnixStream::pair().unwrap();
    let server = std::thread::spawn(move || {
        let mut reader = BufReader::new(theirs.try_clone().unwrap());
        let mut writer = theirs;
        let mut source = Named;
        let mut line = Vec::new();
        while reader.read_until(b'\n', &mut line).unwrap() > 0 {
            writer.write_all(serve_line(&mut source, &line).as_bytes()).unwrap();
            line.clear();
        }
    });
    let mut client = WireSource::new(BufReader::new(ours.try_clone().unwrap()), ours);
    let h = client.hello("podrunner", "0").unwrap();
    assert_eq!(h, Hello { source: "fake".into(), operator_controlled: true });
    let job = JobId::new("j-1").unwrap();
    client.ack(&job).unwrap();
    assert_eq!(client.claim(&job, "r").unwrap_err().code, SourceCode::AlreadyClaimed);
    assert_eq!(client.next(1, Duration::ZERO).unwrap_err().code, SourceCode::Unavailable);
    drop(client);
    server.join().unwrap();
}
