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

/// The fix when a failed job gave the made-up `github.token` to a real API
/// (T-254), else nothing.
pub(super) fn token_hint<J>(logs: &[(J, String)]) -> String {
    match logs.iter().any(|(_, log)| refused(log)) {
        true => format!("{TOKEN_HINT}\n"),
        false => String::new(),
    }
}

/// A real API refuses the made-up token: the API with "Bad credentials",
/// raw.githubusercontent.com with a 404 (the manifest of setup-python,
/// T-269).
fn refused(log: &str) -> bool {
    log.contains("Bad credentials") || log.contains("Unable to get manifest from https://raw.githubusercontent.com")
}

const TOKEN_HINT: &str = "podrunner: a job gave the made-up github.token to a real API, which refused it; fix: give a token with --github-token-env NAME (a token with read access to public repositories is enough); an empty NAME sends no token, which the manifests of setup-* take, but an action that requires a token input (actions/github-script) refuses it";

/// `text` with each secret value as `***`.
pub(super) fn mask(text: &str, secrets: &[String]) -> String {
    // The longest first: a secret that starts another must not leave the
    // rest of the longer one (#228).
    let mut sorted: Vec<&String> = secrets.iter().filter(|s| !s.is_empty()).collect();
    sorted.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let mut out = text.to_string();
    for s in sorted {
        out = out.replace(s.as_str(), "***");
    }
    out
}

/// How long a run is quiet before a line says that it still runs (T-224).
const QUIET: std::time::Duration = std::time::Duration::from_secs(60);

/// The live sink: one line on stderr for each line of a log. After each
/// `QUIET` with no line, a line says that the run still waits, so a run
/// that hangs (a runner that does not reach its broker, a long step with
/// no output) is not silent until its timeout (T-224).
pub(super) fn live(secrets: Vec<String>, as_json: bool) -> Live {
    let last = Arc::new(std::sync::Mutex::new(std::time::Instant::now()));
    // The sink holds the token; the watcher ends when the sink goes.
    let token = Arc::new(());
    watch_quiet(Arc::downgrade(&token), last.clone(), as_json);
    // The fix of a refused token comes at once, not after the run (T-305).
    let hinted = std::sync::atomic::AtomicBool::new(false);
    // The colors of the runner go to a terminal only (T-323 F19).
    let tty = std::io::IsTerminal::is_terminal(&std::io::stderr()) && !as_json;
    Arc::new(move |job: &JobId, text: &str| {
        let _ = &token;
        *last.lock().unwrap_or_else(|e| e.into_inner()) = std::time::Instant::now();
        let text = if tty { text.to_string() } else { plain(text) };
        for line in mask(&text, &secrets).lines() {
            if refused(line) && !hinted.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!("{TOKEN_HINT}");
            }
            if as_json {
                eprintln!("{}", json!({ "event": "log", "job": job.to_string(), "line": line }));
            } else {
                eprintln!("[{job}] {line}");
            }
        }
    })
}

/// `text` with no ANSI escape sequence (CSI: `ESC [` ... a final byte).
pub(super) fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' && it.peek() == Some(&'[') {
            it.next();
            for d in it.by_ref() {
                if ('@'..='~').contains(&d) {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Write a line for each `QUIET` with no line of a job, while `token` lives.
fn watch_quiet(token: std::sync::Weak<()>, last: Arc<std::sync::Mutex<std::time::Instant>>, as_json: bool) {
    let start = std::time::Instant::now();
    std::thread::spawn(move || {
        while token.upgrade().is_some() {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let mut l = last.lock().unwrap_or_else(|e| e.into_inner());
            if l.elapsed() < QUIET || token.upgrade().is_none() {
                continue;
            }
            *l = std::time::Instant::now();
            let s = start.elapsed().as_secs();
            match as_json {
                true => eprintln!("{}", json!({ "event": "quiet", "seconds": QUIET.as_secs(), "run_seconds": s })),
                false => eprintln!(
                    "podrunner: no line for {} s; the run waits for its job ({s} s so far; Ctrl-C ends it)",
                    QUIET.as_secs()
                ),
            }
        }
    });
}

/// One JSON object for the end of a job.
pub(super) fn line_json(l: &Line, dry: bool) -> String {
    let v = match l {
        // A dry run runs no job: a job that would start is `planned` (T-333).
        Line::Ran { job, name, result } => {
            let result =
                if dry && *result == pod_actions_workflow::JobResult::Success { "planned" } else { result.as_str() };
            json!({ "event": "job", "job": job.to_string(), "name": name, "result": result })
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
/// rest, and `DIR/JOB/summary.md` of the summaries of the job. A job that
/// sent no log (it failed before it ran) gets its log of the run as
/// `job.log`, with the cause (T-349).
pub(super) fn write_dir(
    dir: &Path,
    blobs: &[(JobId, Vec<(String, String)>)],
    logs: &[(JobId, String)],
    secrets: &[String],
) -> Result<(), String> {
    for (job, log) in logs.iter().filter(|(j, l)| !l.trim().is_empty() && !blobs.iter().any(|(b, _)| b == j)) {
        let d = dir.join(job.as_str());
        std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        let f = d.join("job.log");
        std::fs::write(&f, mask(log, secrets)).map_err(|e| format!("{}: {e}", f.display()))?;
    }
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

/// Write each result line to stdout when its job ends, not in the `Out` of
/// the run: the binary sets it; a test reads the lines in `Out` (T-296).
pub static STREAM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Add `text` to `out`, or with [`STREAM`] write it to stdout at once.
pub(super) fn emit(out: &mut String, text: &str) {
    if STREAM.load(std::sync::atomic::Ordering::SeqCst) {
        use std::io::Write;
        let mut o = std::io::stdout().lock();
        let _ = o.write_all(text.as_bytes());
        let _ = o.flush();
    } else {
        out.push_str(text);
    }
}

/// The end of stderr of a failed run. The live lines gave each log once;
/// with `-q`, the logs come here (T-289), as JSON with `--json` (T-296).
pub(super) fn failed_end<J: std::fmt::Display>(
    logs: &[(J, String)],
    quiet: bool,
    json: bool,
    dry: bool,
    log_dir: Option<&Path>,
) -> String {
    let mut out = String::new();
    match (quiet, json) {
        (true, true) => {
            for (job, log) in logs {
                for l in log.lines() {
                    out.push_str(&format!("{}\n", json!({ "event": "log", "job": job.to_string(), "line": l })));
                }
            }
        }
        (true, false) => {
            for (job, log) in logs {
                out.push_str(&format!("--- log of {job}\n{log}\n"));
            }
        }
        // The JSON line of each job has its result; no text line (T-339).
        (false, _) if dry || json => {}
        // No log: each failed job ended before it ran, with its reason on
        // its line.
        (false, false) if logs.iter().all(|(_, l)| l.trim().is_empty()) => {
            out.push_str("podrunner: a job failed before it ran; its line above names why\n")
        }
        // The directory of the logs only when the run wrote one (T-349).
        (false, _) => match log_dir {
            Some(d) => out.push_str(&format!("podrunner: a job failed; its log is above and in {}\n", d.display())),
            None => out.push_str("podrunner: a job failed; its log is above\n"),
        },
    }
    out
}

#[cfg(test)]
mod mask_tests {
    /// #228: a secret that starts a longer one leaves no part of it.
    #[test]
    fn the_longest_secret_is_masked_first() {
        let secrets = ["abc".to_string(), "abcdef".to_string()];
        assert_eq!(super::mask("value abcdef end abc", &secrets), "value *** end ***");
    }
}

#[cfg(test)]
mod plain_tests {
    /// T-323 F19: the colors of the runner leave a line for a file.
    #[test]
    fn a_line_with_no_colors() {
        assert_eq!(super::plain("\u{1b}[36;1mecho hi\u{1b}[0m"), "echo hi");
        assert_eq!(super::plain("no codes [x]"), "no codes [x]");
    }
}

#[cfg(test)]
mod hint_tests {
    /// T-254: a failed job with "Bad credentials" in its log names the fix.
    #[test]
    fn bad_credentials_name_the_token_flag() {
        let logs = vec![("a".to_string(), "##[error]Last error: Bad credentials".to_string())];
        assert!(super::token_hint(&logs).contains("--github-token-env"));
        assert!(super::token_hint(&[("a".to_string(), "other".to_string())]).is_empty());
        let python = "##[error]Failed to fetch the Python versions manifest. Unable to get manifest from https://raw.githubusercontent.com/actions/python-versions/main/versions-manifest.json";
        assert!(super::token_hint(&[("a".to_string(), python.to_string())]).contains("an empty NAME"));
    }
}
