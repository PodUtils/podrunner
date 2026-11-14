//! The logs of a local run (T-134): each new part of a log as it comes, on
//! stderr with the job as its prefix (or as JSON with `--json`); `--log-dir
//! DIR` keeps the log of each job and step and the summary of each job.
//! podrunner masks each secret again before it writes a line: the runner
//! masks them too, and a secret must never reach a file of the user.

use std::path::Path;
use std::sync::Arc;

use podrunner_local::drive::Line;
use podrunner_local::exec::Live;
use podrunner_schema::JobId;
use serde_json::json;

/// `text` with each secret value as `***`.
pub(super) fn mask(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for s in secrets.iter().filter(|s| !s.is_empty()) {
        out = out.replace(s.as_str(), "***");
    }
    out
}

/// The live sink: one line on stderr for each line of a log.
pub(super) fn live(secrets: Vec<String>, as_json: bool) -> Live {
    Arc::new(move |job: &JobId, text: &str| {
        for line in mask(text, &secrets).lines() {
            if as_json {
                eprintln!("{}", json!({ "event": "log", "job": job.to_string(), "line": line }));
            } else {
                eprintln!("[{job}] {line}");
            }
        }
    })
}

/// One JSON object for the end of a job.
pub(super) fn line_json(l: &Line) -> String {
    let v = match l {
        Line::Ran { job, name, result } => {
            json!({ "event": "job", "job": job.to_string(), "name": name, "result": result.as_str() })
        }
        Line::Refused { job, name, reason } => {
            json!({ "event": "job", "job": job.to_string(), "name": name, "result": "failure", "reason": reason })
        }
        Line::Skipped { job, condition } => {
            json!({ "event": "job", "name": job, "result": "skipped", "if": condition })
        }
        Line::Kept { job, name, result } => {
            json!({ "event": "job", "job": job.to_string(), "name": name, "result": result.as_str(), "kept": true })
        }
        Line::NotSelected { job, name, reason } => {
            json!({ "event": "job", "job": job.to_string(), "name": name, "result": "skipped", "reason": reason })
        }
    };
    format!("{v}\n")
}

/// Write `DIR/JOB/step-ID.log` for each step log, `DIR/JOB/job.log` for the
/// rest, and `DIR/JOB/summary.md` of the summaries of the job.
pub(super) fn write_dir(
    dir: &Path,
    blobs: &[(JobId, Vec<(String, String)>)],
    secrets: &[String],
) -> Result<(), String> {
    for (job, list) in blobs {
        let d = dir.join(job.as_str());
        std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        let mut summary = String::new();
        let mut rest = String::new();
        for (name, text) in list {
            let text = mask(text, secrets);
            if name.starts_with("summary-") {
                summary.push_str(&text);
            } else if name.starts_with("step-") && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                let f = d.join(format!("{name}.log"));
                std::fs::write(&f, text).map_err(|e| format!("{}: {e}", f.display()))?;
            } else {
                rest.push_str(&text);
            }
        }
        for (file, text) in [("summary.md", summary), ("job.log", rest)] {
            if !text.is_empty() {
                let f = d.join(file);
                std::fs::write(&f, text).map_err(|e| format!("{}: {e}", f.display()))?;
            }
        }
    }
    Ok(())
}
