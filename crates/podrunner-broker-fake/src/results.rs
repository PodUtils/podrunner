//! The results service of the logs (T-119, T-134): a signed URL is a blob of
//! this broker; each upload appends; each metadata call is `ok`. The
//! signature of a blob URL is random for each broker (T-406): a local
//! process cannot write lines into the log of a job.

use std::sync::Mutex;

use serde_json::{Value, json};

use crate::{State, reply};

/// The answer to a call of the results service, or none for another route.
/// `full` is the path with its query.
pub(crate) fn route(
    method: &str,
    path: &str,
    full: &str,
    body: &Value,
    raw: &[u8],
    base: &str,
    state: &Mutex<State>,
) -> Option<(u16, Vec<u8>)> {
    let receiver = "/twirp/results.services.receiver.Receiver/";
    match (method, path) {
        ("POST", p) if p.starts_with(&format!("{receiver}Get")) => {
            // A summary of `GITHUB_STEP_SUMMARY` is a blob of its own (T-134).
            let kind = if p.contains("Summary") { "summary" } else { "step" };
            let name = match body["step_backend_id"].as_str() {
                Some(step) => format!("{kind}-{step}"),
                None => p.rsplit('/').next().unwrap_or("blob").to_string(),
            };
            let sig = state.lock().expect("the broker").auth.blob.clone();
            let url = format!("{base}blobs/{name}?sig={sig}");
            Some(reply(&json!({
                "logs_url": url, "summary_url": url, "blob_storage_type": "BLOB_STORAGE_TYPE_UNSPECIFIED",
                "soft_size_limit": 1u64 << 30,
            })))
        }
        ("POST", p) if p.starts_with(&format!("{receiver}Create")) => Some(reply(&json!({ "ok": true }))),
        ("POST", "/twirp/github.actions.results.api.v1.WorkflowStepUpdateService/WorkflowStepsUpdate") => {
            state.lock().expect("the broker").step_updates.push(body.clone());
            Some(reply(&json!({ "ok": true })))
        }
        ("PUT", p) if p.starts_with("/blobs/") => {
            let name = p.trim_start_matches("/blobs/").to_string();
            let sig = full.split_once('?').map_or("", |(_, q)| q).split('&').find_map(|kv| kv.strip_prefix("sig="));
            let mut st = state.lock().expect("the broker");
            if !sig.is_some_and(|s| st.auth.blob_sig(s)) {
                return Some((403, b"the signature of the blob URL is wrong".to_vec()));
            }
            st.blobs.entry(name).or_default().extend_from_slice(raw);
            Some((201, Vec::new()))
        }
        _ => None,
    }
}
