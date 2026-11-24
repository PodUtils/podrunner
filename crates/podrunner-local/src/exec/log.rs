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
    job_log(broker)
}

/// The log of a job: the logs that the runner uploaded, and one line for
/// each request that the broker had no route for (T-145).
fn job_log(broker: &Broker) -> String {
    let mut log = broker.logs();
    for m in broker.missing() {
        log.push_str(&format!("\n{NO_ROUTE}{m}"));
    }
    log
}

/// The last 8 KiB of an output, as text.
pub(super) fn tail(out: &[u8]) -> String {
    let start = out.len().saturating_sub(8 * 1024);
    String::from_utf8_lossy(&out[start..]).into_owned()
}
