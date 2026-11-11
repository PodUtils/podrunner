//! The harness against the stub driver: each test fails, and each is
//! reported by its number. The contract names the operations of the trait.

use podrunner_schema::{UnitId, UnitIds};

use crate::conformance::{self, Behaviour, Fixture, Outcome};
use crate::{OPERATIONS, Stub, UnitCode, UnitDriver, parse_pinned};

struct StubFixture {
    stub: Stub,
    ids: UnitIds,
}

impl Fixture for StubFixture {
    fn driver(&self) -> &dyn UnitDriver {
        &self.stub
    }
    fn image(&self) -> String {
        format!("stand-in@sha256:{}", "0".repeat(64))
    }
    fn platform(&self) -> String {
        "linux/amd64".into()
    }
    fn foreign_platform(&self) -> String {
        "linux/s390x".into()
    }
    fn payload(&self, _: Behaviour) -> Vec<String> {
        vec!["true".into()]
    }
    fn unit(&self) -> UnitId {
        self.ids.next()
    }
}

#[test]
fn the_harness_fails_each_test_on_the_stub() {
    let fx = StubFixture { stub: Stub, ids: UnitIds::default() };
    let results = conformance::run(&fx, &[]);
    let numbers: Vec<u32> = results.iter().map(|r| r.number).collect();
    assert_eq!(numbers, (1..=26).collect::<Vec<_>>());
    for r in &results {
        assert!(matches!(r.outcome, Outcome::Fail(_)), "{r}");
        assert!(r.to_string().starts_with(&format!("unit {:2} {}: FAIL", r.number, r.name)), "{r}");
    }
    assert_eq!(conformance::not_passed(&results).len(), 26);
}

#[test]
fn only_the_named_tests_run() {
    let fx = StubFixture { stub: Stub, ids: UnitIds::default() };
    let results = conformance::run(&fx, &[3, 12]);
    assert_eq!(results.iter().map(|r| r.number).collect::<Vec<_>>(), [3, 12]);
}

#[test]
fn a_pinned_reference_has_a_full_digest() {
    let hex = "a".repeat(64);
    assert_eq!(parse_pinned(&format!("img@sha256:{hex}")).unwrap(), ("img", format!("sha256:{hex}").as_str()));
    for bad in
        ["img:latest", "img", "img@sha256:abc", &format!("img@sha256:{}", "A".repeat(64)), &format!("@sha256:{hex}")]
    {
        assert_eq!(parse_pinned(bad).unwrap_err().code, UnitCode::PayloadNotPinned, "{bad}");
    }
}

/// The contract names each operation of the trait, in the same order, and
/// each test of the suite.
#[test]
fn the_contract_names_the_operations_and_the_tests() {
    let doc = include_str!("../../../docs/contracts/unit-driver.md");
    let section = doc.split("\n## Operations\n").nth(1).expect("a section Operations").split("\n## ").next().unwrap();
    let ops: Vec<&str> = section
        .lines()
        .filter_map(|l| l.strip_prefix("| `"))
        .filter_map(|l| l.split_once('`').map(|(op, _)| op))
        .collect();
    assert_eq!(ops, OPERATIONS, "the table of operations of docs/contracts/unit-driver.md");
    for (n, name) in conformance::tests() {
        let row = format!("| {n} | `{name}` |");
        assert!(doc.contains(&row), "docs/contracts/unit-driver.md has no row {row}");
    }
}
