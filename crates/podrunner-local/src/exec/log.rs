//! The log of a job of the executors: the live lines, the blobs and the
//! text of the log (T-134, T-145).

use podrunner_broker_fake::Broker;
use podrunner_schema::JobId;

use super::{Live, NO_ROUTE};

/// Give the new text of each log blob to the live sink.
pub(super) fn pump(live: &Option<Live>, job: &JobId, broker: &Broker) {
    if let Some(l) = live {
        for (_, text) in broker.take_new() {
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
    blobs.push((job.clone(), broker.blobs()));
    // One line for each request that the broker had no route for (T-145).
    let missing: String = broker.missing().iter().map(|m| format!("\n{NO_ROUTE}{m}")).collect();
    added(blobs, job, &missing);
    broker.logs() + &missing
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
