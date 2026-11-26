//! T-642: each example of the script of the fake source in `docs/fakes.md`
//! and the file `docs/examples/fake-jobs.jsonl` read, and each offer in
//! them is a complete `job-offer.v1`.

use std::path::Path;

use podrunner_fake::parse_script;
use serde_json::Value;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn offers_read(line: &str) {
    parse_script(line).unwrap_or_else(|e| panic!("{e}: {line}"));
    let v: Value = serde_json::from_str(line).unwrap();
    if let Some(offer) = v.get("offer") {
        let o: podrunner_schema::JobOffer =
            serde_json::from_value(offer.clone()).unwrap_or_else(|e| panic!("{e}: {line}"));
        o.check(false).unwrap_or_else(|e| panic!("{e}: {line}"));
    }
}

#[test]
fn the_examples_of_the_script_are_complete() {
    let doc = std::fs::read_to_string(root().join("docs/fakes.md")).unwrap();
    let lines: Vec<&str> = doc.split('`').filter(|s| s.starts_with("{\"at_s\"")).collect();
    assert_eq!(lines.len(), 5, "{lines:?}");
    lines.into_iter().for_each(offers_read);
    let file = std::fs::read_to_string(root().join("docs/examples/fake-jobs.jsonl")).unwrap();
    let jobs: Vec<&str> = file.lines().filter(|l| l.starts_with('{')).collect();
    assert_eq!(jobs.len(), 1);
    jobs.into_iter().for_each(offers_read);
}
