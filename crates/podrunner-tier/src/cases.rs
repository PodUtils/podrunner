//! The published case table of the tier rule, `tier-cases.json` (T-518,
//! githome's G-13): the conformance harness of the seam [`TrustRule`].
//!
//! `tests/cases.json` holds the cases as people write them: one base offer,
//! the fields that each row changes, and the trust lists of the table or of
//! the row. [`published`] makes each row whole, so that a program in any
//! language can run it with no merge: the trust lists, the tier of the set,
//! the facts of the job, and the result. A test holds `tier-cases.json` in
//! step with this code; each release attaches it.

use podrunner_schema::offer::{Event, TrustInputs};
use podrunner_schema::{RefusalCode, Tier};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{EXTRA_EVENTS, TRUSTED_EVENTS, TierConfig, TrustRule, event_name};

/// The events that no list can make trusted.
const NEVER_TRUSTED: [Event; 2] = [Event::Other, Event::Unknown];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    config: TierConfig,
    base: Map<String, Value>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    set: Tier,
    with: Map<String, Value>,
    /// The trust lists of this row, in place of those of the table.
    #[serde(default)]
    trust: Option<TierConfig>,
    expect: String,
}

/// One whole row of the published table.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    name: String,
    set: Tier,
    config: TierConfig,
    trust_inputs: TrustInputs,
    expect: String,
}

#[derive(Deserialize)]
struct Published {
    schema: u32,
    cases: Vec<Row>,
}

/// The result of a rule as the table writes it: the tier, or the code of
/// the refusal.
pub fn show(r: Result<Tier, RefusalCode>) -> String {
    match r {
        Ok(t) => serde_json::to_value(t).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
        Err(c) => c.as_str().to_string(),
    }
}

fn names(events: &[Event]) -> Vec<String> {
    events.iter().map(|e| event_name(*e)).collect()
}

/// The published table (`tier-cases.json`) of the hand-written table
/// `table` (the text of `tests/cases.json`).
pub fn published(table: &str) -> Result<String, String> {
    let t: Table = serde_json::from_str(table).map_err(|e| format!("the case table: {e}"))?;
    let mut rows = Vec::new();
    for c in t.cases {
        let mut inputs = t.base.clone();
        inputs.extend(c.with);
        // A field that the offer does not know fails here, not in a reader.
        let inputs: TrustInputs =
            serde_json::from_value(Value::Object(inputs)).map_err(|e| format!("{}: {e}", c.name))?;
        let config = c.trust.unwrap_or_else(|| t.config.clone());
        config.check().map_err(|e| format!("{}: {e}", c.name))?;
        rows.push(json!({
            "name": c.name,
            "set": c.set,
            "config": config,
            "trust_inputs": inputs,
            "expect": c.expect,
        }));
    }
    let v = json!({
        "schema": 1,
        "kind": "tier-cases",
        "about": "The cases of the tier rule of podrunner (crates/podrunner-tier). Each row gives the trust lists \
                  (config: the keys of [trust] of podrunner.toml), the tier of the set that delivered the job \
                  (set), the facts of the job (trust_inputs of job-offer.v1) and the result (expect): the tier \
                  that the job runs under, or the code of the refusal. A rule passes when it gives expect for \
                  each row.",
        "rule": {
            "trusted_events": names(&TRUSTED_EVENTS),
            "extra_events": names(&EXTRA_EVENTS),
            "never_trusted": names(&NEVER_TRUSTED),
            "untrusted_fork_events": names(&[Event::PullRequest]),
        },
        "cases": rows,
    });
    serde_json::to_string_pretty(&v).map(|s| s + "\n").map_err(|e| e.to_string())
}

/// The rows of the published table `text` that `rule` gets wrong, each as
/// `NAME: GOT, not EXPECT`; an empty list when the rule passes.
pub fn conformance(rule: &dyn TrustRule, text: &str) -> Result<Vec<String>, String> {
    let p: Published = serde_json::from_str(text).map_err(|e| format!("tier-cases.json: {e}"))?;
    if p.schema != 1 {
        return Err(format!("tier-cases.json: schema {}, not 1", p.schema));
    }
    Ok(p.cases
        .iter()
        .filter_map(|r| {
            let got = show(rule.intake(&r.trust_inputs, &r.config, r.set));
            (got != r.expect).then(|| format!("{}: {got}, not {}", r.name, r.expect))
        })
        .collect())
}

/// The count of rows of the published table `text`.
pub fn count(text: &str) -> Result<usize, String> {
    let p: Published = serde_json::from_str(text).map_err(|e| format!("tier-cases.json: {e}"))?;
    Ok(p.cases.len())
}
