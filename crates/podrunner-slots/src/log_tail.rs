//! The `log_tail` of a run report (T-497, githome's G-19): the end of the
//! log of a unit that failed before its runner ran a step, with each
//! credential scrubbed.
//!
//! slots keeps no credential after a start: it keeps the SHA-256 of each
//! string value of a JSON credential file (each long word of another
//! file), and masks each word of the log whose hash is one of them. Words
//! of the shape of a token of GitHub are masked too, from any source.

use podrunner_schema::Credential;
use ring::digest::{SHA256, digest};

/// The mask of a scrubbed word.
pub const MASK: &str = "***";

/// The shortest word of a credential file that is not JSON that is a
/// mark: a shorter part is a name or a number, not a secret.
const MIN_WORD: usize = 12;

/// The shortest string value of a JSON credential file that is a mark.
const MIN_VALUE: usize = 8;

/// The files of a unit that hold no secret: the name and the pool of the
/// runner, and the job that podrunner placed (T-175).
const PUBLIC_FILES: [&str; 2] = [".runner", ".podrunner-job"];

/// The hashes of the long words of the credential of one job.
#[derive(Debug, Clone, Default)]
pub struct Marks(Vec<[u8; 32]>);

fn word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'_' | b'-' | b'.')
}

fn hash(word: &[u8]) -> [u8; 32] {
    digest(&SHA256, word).as_ref().try_into().expect("32 bytes")
}

/// Each word of `text`, with its start.
fn words(text: &[u8]) -> impl Iterator<Item = (usize, &[u8])> {
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < text.len() && !word_byte(text[i]) {
            i += 1;
        }
        let start = i;
        while i < text.len() && word_byte(text[i]) {
            i += 1;
        }
        (start < text.len()).then(|| (start, &text[start..i]))
    })
}

impl Marks {
    /// The marks of each file of `credential`.
    pub fn of(credential: &Credential) -> Marks {
        let mut v: Vec<[u8; 32]> = Vec::new();
        for f in credential.files.iter().filter(|f| !PUBLIC_FILES.contains(&f.name.as_str())) {
            let text = f.content.expose();
            match serde_json::from_str::<serde_json::Value>(text) {
                Ok(json) => strings(&json, &mut |s| {
                    if s.len() >= MIN_VALUE {
                        v.push(hash(s.as_bytes()));
                    }
                }),
                Err(_) => v.extend(words(text.as_bytes()).filter(|(_, w)| w.len() >= MIN_WORD).map(|(_, w)| hash(w))),
            }
        }
        v.sort_unstable();
        v.dedup();
        Marks(v)
    }

    fn holds(&self, word: &[u8]) -> bool {
        word.len() >= MIN_VALUE && self.0.binary_search(&hash(word)).is_ok()
    }
}

/// Each string value in `v`.
fn strings(v: &serde_json::Value, f: &mut impl FnMut(&str)) {
    match v {
        serde_json::Value::String(s) => f(s),
        serde_json::Value::Array(a) => a.iter().for_each(|x| strings(x, f)),
        serde_json::Value::Object(o) => o.values().for_each(|x| strings(x, f)),
        _ => {}
    }
}

/// A word of the shape of a token of GitHub: an installation, user, OAuth
/// or refresh token, a fine-grained token, or a JSON web token.
fn token_shape(word: &[u8]) -> bool {
    let w = String::from_utf8_lossy(word);
    ["ghs_", "ghp_", "gho_", "ghu_", "ghr_", "github_pat_"].iter().any(|p| w.starts_with(p) && w.len() > p.len() + 8)
        || (w.starts_with("eyJ") && w.matches('.').count() >= 2)
}

/// The line that the job-started hook of the runner image
/// (`images/runner/podrunner-job-started.sh`) writes to the stderr of the
/// runner when it ends a job before its first step (T-552): the runner
/// exits 0 after such a job, so its exit code does not tell.
pub const HOOK_ENDED: &str = "podrunner-job-started: ended: ";

/// The code of the last [`HOOK_ENDED`] line of `log`: at most 64 bytes of
/// `[a-z0-9-]`, else `end`.
pub fn hook_ended(log: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(log);
    let rest = text.lines().rev().find_map(|l| l.find(HOOK_ENDED).map(|i| &l[i + HOOK_ENDED.len()..]))?;
    let code: String =
        rest.chars().take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-').take(64).collect();
    Some(if code.is_empty() { "end".into() } else { code })
}

/// Lines that only the official runner writes to the log of its unit:
/// `run-helper.sh` at each end of `Runner.Listener`, and the listener at
/// its session. A payload that is not the runner (a sandbox, a script of
/// a test) writes none (T-567).
const RUNNER_LINES: [&str; 4] =
    ["Runner listener exit with ", "Exiting with unknown error code", ": Listening for Jobs", "Connected to GitHub"];

/// Lines that the runner writes when it takes a job (`JobDispatcher.cs`
/// of actions/runner): at the start and at the end of the job.
const JOB_LINES: [&str; 2] = ["Running job: ", " completed with result: "];

/// True when `log` shows the official runner and no job of it (T-567).
/// `run.sh` exits 0 also when the listener ends with code 1 (a refused
/// session), so its exit code does not tell that a job ran.
pub fn runner_took_no_job(log: &[u8]) -> bool {
    let text = String::from_utf8_lossy(log);
    RUNNER_LINES.iter().any(|l| text.contains(l)) && !JOB_LINES.iter().any(|l| text.contains(l))
}

/// `log` with each mark and each token shape masked: its last
/// [`podrunner_schema::report::LOG_TAIL_MAX`] bytes at most, as UTF-8.
pub fn scrub(log: &[u8], marks: &Marks) -> String {
    let mut out = Vec::with_capacity(log.len());
    let mut at = 0;
    for (start, w) in words(log) {
        out.extend_from_slice(&log[at..start]);
        if marks.holds(w) || token_shape(w) {
            out.extend_from_slice(MASK.as_bytes());
        } else {
            out.extend_from_slice(w);
        }
        at = start + w.len();
    }
    out.extend_from_slice(&log[at..]);
    let text = String::from_utf8_lossy(&out);
    let max = podrunner_schema::report::LOG_TAIL_MAX;
    if text.len() <= max {
        return text.into_owned();
    }
    let mut cut = text.len() - max;
    while !text.is_char_boundary(cut) {
        cut += 1;
    }
    text[cut..].to_string()
}

#[cfg(test)]
mod tests {
    use podrunner_schema::{CredentialFile, Secret};

    use super::*;

    fn cred(text: &str) -> Credential {
        Credential {
            files: vec![CredentialFile { name: ".credentials".into(), content: Secret::new(text.to_string()) }],
        }
    }

    #[test]
    fn a_word_of_the_credential_is_masked_and_a_short_one_is_not() {
        let marks = Marks::of(&cred(r#"{"scheme":"OAuth","data":{"token":"canary-made-up-Qz81hX0pLm"}}"#));
        let log = b"start\nAuthorization: Bearer canary-made-up-Qz81hX0pLm\nscheme OAuth done\n";
        let s = scrub(log, &marks);
        assert!(!s.contains("canary-made-up"), "{s}");
        assert!(s.contains("Bearer ***") && s.contains("scheme OAuth done"), "{s}");
    }

    #[test]
    fn a_value_of_a_json_file_is_masked_and_a_public_file_is_not() {
        let mut c = cred(r#"{"data":{"clientId":"fake-client"}}"#);
        c.files.push(CredentialFile { name: ".runner".into(), content: Secret::new(r#"{"agentName":"runner-u-1"}"#) });
        let s = scrub(b"client fake-client of runner-u-1\n", &Marks::of(&c));
        assert_eq!(s, "client *** of runner-u-1\n");
    }

    #[test]
    fn a_token_of_github_is_masked_with_no_mark() {
        let s = scrub(b"x ghs_madeupmadeupmadeup1234 y eyJhbGciOi.eyJzdWIiOi.c2lnbmF0dXJl z", &Marks::default());
        assert_eq!(s, "x *** y *** z");
    }

    #[test]
    fn the_line_of_the_hook_gives_its_code() {
        let log = format!("x\n{HOOK_ENDED}no-answer\ny\n{HOOK_ENDED}source-unavailable; more\n");
        assert_eq!(hook_ended(log.as_bytes()).as_deref(), Some("source-unavailable"));
        assert_eq!(hook_ended(format!("{HOOK_ENDED}\n").as_bytes()).as_deref(), Some("end"));
        assert_eq!(hook_ended(b"podrunner-job-started: kept\n"), None);
    }

    #[test]
    fn the_runner_with_no_job_line_took_no_job() {
        let exit = "Runner listener exit with terminated error, stop the service, no retry needed.\n";
        assert!(runner_took_no_job(exit.as_bytes()));
        let ran = format!("2026-11-20 10:00:00Z: Running job: build\n{exit}");
        assert!(!runner_took_no_job(ran.as_bytes()));
        let done = format!("2026-11-20 10:00:09Z: Job build completed with result: Succeeded\n{exit}");
        assert!(!runner_took_no_job(done.as_bytes()));
        // No line of the runner: not the runner, so no rule.
        assert!(!runner_took_no_job(b"echo fine\n"));
    }

    #[test]
    fn the_tail_is_the_end_of_the_log_at_most_the_bound() {
        let max = podrunner_schema::report::LOG_TAIL_MAX;
        let mut log = "é".repeat(max).into_bytes();
        log.extend_from_slice(b"\nthe end\n");
        let s = scrub(&log, &Marks::default());
        assert!(s.len() <= max && s.ends_with("the end\n"), "{}", s.len());
    }
}
