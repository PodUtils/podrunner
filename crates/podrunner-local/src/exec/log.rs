//! The log of a job of the executors: the live lines, the blobs and the
//! text of the log (T-134, T-145).

use std::collections::BTreeMap;

use podrunner_broker_fake::Broker;
use podrunner_schema::JobId;

use super::{Live, NO_ROUTE};

/// Give the new text of each log blob to the live sink: the feed, then
/// each step log in the order of its step, not of its random id (T-308
/// A11, A12: a guest has no feed, and steps that end in one pump came in
/// the order of their ids).
pub(super) fn pump(live: &Option<Live>, job: &JobId, broker: &Broker) {
    if let Some(l) = live {
        let mut new = broker.take_new();
        let numbers = step_numbers(&broker.step_updates());
        new.sort_by_key(|(name, _)| if name == "feed" { 0 } else { order(name, &numbers) });
        for (_, text) in new {
            l(job, &text);
        }
    }
}

/// The end of a job: the last live lines, the blobs of the job (its step
/// logs and summaries, T-134), and its log.
pub(super) fn record(
    live: &Option<Live>,
    blobs: &mut Vec<(JobId, Vec<(String, String)>)>,
    job: &JobId,
    broker: &Broker,
) -> String {
    pump(live, job, broker);
    let numbers = step_numbers(&broker.step_updates());
    let all = broker.blobs();
    let log = job_log(&all, &numbers);
    // A step log takes the number of its step: the files sort in the order
    // of the steps (T-308 A28). A number of two steps names neither.
    let count = |n: u64| numbers.values().filter(|m| **m == n).count();
    let renamed = |name: &str| match order(name, &numbers) {
        n if n != u64::MAX && count(n) == 1 => Some(format!("step-{n:02}")),
        _ => None,
    };
    blobs.push((job.clone(), all.into_iter().map(|(n, t)| (renamed(&n).unwrap_or(n), t)).collect()));
    // One line for each request that the broker had no route for (T-145).
    let missing: String = broker.missing().iter().map(|m| format!("\n{NO_ROUTE}{m}")).collect();
    added(blobs, job, &missing);
    log + &missing
}

/// The number of each step by its id in lower case, from the step updates
/// of the Worker; the last update of a step wins.
fn step_numbers(updates: &[serde_json::Value]) -> BTreeMap<String, u64> {
    let mut by_id = BTreeMap::new();
    for s in updates.iter().filter_map(|u| u.get("steps")?.as_array()).flatten() {
        let id = s.get("external_id").or_else(|| s.get("externalId")).and_then(serde_json::Value::as_str);
        if let (Some(id), Some(n)) = (id, s.get("number").and_then(serde_json::Value::as_u64)) {
            by_id.insert(id.to_ascii_lowercase(), n);
        }
    }
    by_id
}

/// The number of the step of the blob `name` (`step-ID`), else the largest
/// number: such a blob comes after the steps.
fn order(name: &str, numbers: &BTreeMap<String, u64>) -> u64 {
    name.strip_prefix("step-").and_then(|id| numbers.get(&id.to_ascii_lowercase())).copied().unwrap_or(u64::MAX)
}

/// The text of the log of a job: the log of the whole job that the Worker
/// uploaded, else each step log in the order of its step (T-282). The
/// name of a step can hold any text: it goes into no file name.
fn job_log(blobs: &[(String, String)], numbers: &BTreeMap<String, u64>) -> String {
    let text = |b: &String| b.replace('\u{feff}', "");
    let whole: Vec<String> =
        blobs.iter().filter(|(k, _)| !k.starts_with("summary-") && !k.starts_with("step-")).map(|(_, b)| text(b)).collect();
    if !whole.is_empty() {
        return whole.join("\n");
    }
    let mut steps: Vec<&(String, String)> = blobs.iter().filter(|(k, _)| k.starts_with("step-")).collect();
    steps.sort_by_key(|(k, _)| order(k, numbers));
    steps.into_iter().map(|(_, b)| text(b)).collect::<Vec<_>>().join("\n")
}

/// The name of the blob entry with the lines that podrunner adds to the
/// log of a job: `--log-dir` writes them into `job.log` after the lines of
/// the runner (T-723). An HTTP path has no space, so no blob of the runner
/// has this name.
pub const ADDED: &str = "the lines of podrunner";

/// Add `text`, a part of the log of `job` that podrunner wrote, to the
/// [`ADDED`] entry of the last blobs of the job. A job with no blobs gets
/// its whole log in `--log-dir` already.
pub(super) fn added(blobs: &mut [(JobId, Vec<(String, String)>)], job: &JobId, text: &str) {
    let Some((_, list)) = blobs.iter_mut().rev().find(|(j, _)| j == job).filter(|_| !text.is_empty()) else {
        return;
    };
    match list.iter_mut().find(|(n, _)| n == ADDED) {
        Some((_, t)) => t.push_str(text),
        None => list.push((ADDED.to_string(), text.to_string())),
    }
}

/// The last 8 KiB of an output, as text.
pub(super) fn tail(out: &[u8]) -> String {
    let start = out.len().saturating_sub(8 * 1024);
    String::from_utf8_lossy(&out[start..]).into_owned()
}

#[cfg(test)]
mod order_tests {
    use super::{job_log, order, step_numbers};

    /// T-308 A11, A28: a step log comes in the order of its step, not of its
    /// id; the last update of a step wins; a blob with no number comes last.
    #[test]
    fn the_step_logs_come_in_the_order_of_their_steps() {
        let updates = [
            serde_json::json!({"steps": [{"external_id": "FF-1", "number": 1, "name": "Set up"}, {"external_id": "aa-3", "number": 9}]}),
            serde_json::json!({"steps": [{"external_id": "aa-3", "number": 3}, {"external_id": "e"}]}),
        ];
        let numbers = step_numbers(&updates);
        assert_eq!((order("step-ff-1", &numbers), order("step-AA-3", &numbers)), (1, 3));
        assert_eq!((order("step-e", &numbers), order("summary-ff-1", &numbers)), (u64::MAX, u64::MAX));
        let blobs: Vec<(String, String)> = [("step-aa-3", "three"), ("step-e", "last"), ("step-ff-1", "\u{feff}one"), ("summary-x", "s")]
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .to_vec();
        assert_eq!(job_log(&blobs, &numbers), "one\nthree\nlast");
        let with_job = [blobs, vec![("job".to_string(), "whole".to_string())]].concat();
        assert_eq!(job_log(&with_job, &numbers), "whole", "the log of the whole job wins");
    }
}

#[cfg(test)]
mod added_tests {
    use super::{ADDED, added};
    use podrunner_schema::JobId;

    /// T-723: the text goes to the last blobs of its job, one entry; empty
    /// text and a job with no blobs add nothing.
    #[test]
    fn the_added_text_goes_to_the_last_blobs_of_its_job() {
        let (a, b) = (JobId::new("a.0").unwrap(), JobId::new("b.0").unwrap());
        let mut blobs = vec![(a.clone(), vec![]), (a.clone(), vec![("job".to_string(), "x".to_string())])];
        added(&mut blobs, &a, "\none");
        added(&mut blobs, &a, "");
        added(&mut blobs, &a, "\ntwo");
        added(&mut blobs, &b, "\nlost");
        assert!(blobs[0].1.is_empty());
        let entry: Vec<_> = blobs[1].1.iter().filter(|(n, _)| n == ADDED).collect();
        assert_eq!(entry, [&(ADDED.to_string(), "\none\ntwo".to_string())]);
        assert_eq!(blobs.len(), 2);
    }
}
