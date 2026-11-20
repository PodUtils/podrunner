//! One pass over the log of a unit, with a bound of memory (T-570): the
//! lines of the job-started hook (T-552), of the runner (T-567) and of the
//! job (T-568, T-569), and the end of the log for `log_tail` (T-497). The
//! caller gives the log in chunks, as it reads them.

use podrunner_schema::report::LOG_TAIL_MAX;

use super::{HOOK_ENDED, JOB_LINES, JobResult, Marks, RUNNER_LINES, STAMP, word_byte};

/// The most bytes of one part of a line that the scan reads. Of a longer
/// line it reads the start and the end: a line of the listener ends a
/// line that a process of the job started with no new line.
const LINE_PART: usize = 8 * 1024;

/// The bytes of the end of the log that the scan keeps for `log_tail`.
/// The scrub makes a word of 8 bytes or more into 3, so it keeps more
/// than the bound of the tail.
const KEEP: usize = 4 * LOG_TAIL_MAX;

/// The state of the scan of one log.
#[derive(Debug, Default)]
pub struct Scan {
    /// The code of the last [`HOOK_ENDED`] line.
    pub hook: Option<String>,
    runner: bool,
    job_line: bool,
    /// The name of the first `STAMP: Running job: NAME` record.
    running: Option<String>,
    result: Option<JobResult>,
    /// The start of the line in `line`, when the line is longer than
    /// [`LINE_PART`].
    head: Option<Vec<u8>>,
    line: Vec<u8>,
    tail: Vec<u8>,
    /// True when `tail` lost bytes at its start.
    tail_cut: bool,
}

/// The code after a [`HOOK_ENDED`] mark: at most 64 bytes of
/// `[a-z0-9-]`, else `end`.
fn code(rest: &str) -> String {
    let code: String =
        rest.chars().take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-').take(64).collect();
    if code.is_empty() { "end".into() } else { code }
}

/// Each part of `text` that starts with the time stamp of the console of
/// the listener, after the stamp: the whole line, or its end. A line of
/// the trace (`[WORKER ...`) or of the job log (an ISO stamp) has none.
fn records(text: &str) -> impl Iterator<Item = &str> {
    let mark = &STAMP[STAMP.len() - 3..];
    let at = STAMP.len() - mark.len();
    let b = text.as_bytes();
    text.match_indices(std::str::from_utf8(mark).expect("ASCII")).filter_map(move |(i, _)| {
        let start = i.checked_sub(at)?;
        let ok = STAMP.iter().zip(&b[start..]).all(|(s, c)| if *s == b'0' { c.is_ascii_digit() } else { s == c });
        ok.then(|| &text[i + mark.len()..])
    })
}

impl Scan {
    /// The scan of all of `log`.
    pub fn of(log: &[u8]) -> Scan {
        let mut s = Scan::default();
        s.feed(log);
        s.end();
        s
    }

    /// Scan the next bytes of the log.
    pub fn feed(&mut self, mut b: &[u8]) {
        self.keep(b);
        while let Some(i) = b.iter().position(|c| *c == b'\n') {
            self.push(&b[..i]);
            self.line_end();
            b = &b[i + 1..];
        }
        self.push(b);
    }

    /// Scan the last line, when it has no new line.
    pub fn end(&mut self) {
        if !self.line.is_empty() || self.head.is_some() {
            self.line_end();
        }
    }

    /// True when the log shows the official runner and no job of it.
    pub fn took_no_job(&self) -> bool {
        self.runner && !self.job_line
    }

    /// True when the log has a `STAMP: Running job:` line: the runner took
    /// a job.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// The result of the job of the first `Running job:` line. The listener
    /// prints one completed line for the job, so each other one is forged
    /// by a process of the job: a result that gives no exit reason
    /// ([`JobResult::reason`]) does not hide one that does; of those that
    /// do, the last wins.
    pub fn result(&self) -> Option<JobResult> {
        self.result
    }

    /// The end of the log, scrubbed: at most `LOG_TAIL_MAX` bytes.
    pub fn tail(&self, marks: &Marks) -> String {
        let mut t = &self.tail[..];
        if self.tail_cut {
            // The first word can be the end of a credential: no mark holds
            // a part of a word, so it goes.
            t = &t[t.iter().position(|c| !word_byte(*c)).unwrap_or(t.len())..];
        }
        super::scrub(t, marks)
    }

    fn keep(&mut self, b: &[u8]) {
        self.tail.extend_from_slice(b);
        if self.tail.len() > 2 * KEEP {
            self.tail.drain(..self.tail.len() - KEEP);
            self.tail_cut = true;
        }
    }

    fn push(&mut self, b: &[u8]) {
        self.line.extend_from_slice(b);
        if self.head.is_none() && self.line.len() > LINE_PART {
            let rest = self.line.split_off(LINE_PART);
            self.head = Some(std::mem::replace(&mut self.line, rest));
        }
        if self.head.is_some() && self.line.len() > 2 * LINE_PART {
            self.line.drain(..self.line.len() - LINE_PART);
        }
    }

    fn line_end(&mut self) {
        let mut line = std::mem::take(&mut self.line);
        if let Some(head) = self.head.take() {
            self.part(&head);
        }
        self.part(&line);
        line.clear();
        self.line = line;
    }

    fn part(&mut self, b: &[u8]) {
        let raw = String::from_utf8_lossy(b);
        if let Some(i) = raw.find(HOOK_ENDED) {
            self.hook = Some(code(&raw[i + HOOK_ENDED.len()..]));
        }
        self.runner |= RUNNER_LINES.iter().any(|l| raw.contains(l));
        self.job_line |= JOB_LINES.iter().any(|l| raw.contains(l));
        // NAME is compared with no trailing white space: the end of the
        // line is cut, also a `\r`.
        for rest in records(raw.trim_end()) {
            self.record(rest);
        }
    }

    fn record(&mut self, rest: &str) {
        let Some(job) = &self.running else {
            // A JIT runner runs one job, and the listener prints its line
            // before a step runs: the first one binds.
            self.running = rest.strip_prefix("Running job: ").map(|n| n.trim_end().to_string());
            return;
        };
        let Some((name, word)) = rest.strip_prefix("Job ").and_then(|r| r.rsplit_once(" completed with result: "))
        else {
            return;
        };
        if name.trim_end() != job || word.is_empty() || !word.bytes().all(|c| c.is_ascii_alphabetic()) {
            return;
        }
        let r = JobResult::of(word);
        if r.reason().is_some() || self.result.is_none_or(|f| f.reason().is_none()) {
            self.result = Some(r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN: &str = "2026-11-20 10:00:00Z: Running job: build\n";

    fn end(r: &str) -> String {
        format!("2026-11-20 10:00:09Z: Job build completed with result: {r}\n")
    }

    #[test]
    fn each_result_of_the_runner_gives_its_reason_and_abandoned_is_a_failure() {
        use podrunner_schema::ExitReason::{Cancelled, Failure};
        for (word, reason) in [
            ("Succeeded", None),
            ("SucceededWithIssues", None),
            ("Skipped", None),
            ("Failed", Some(Failure)),
            ("Canceled", Some(Cancelled)),
            ("Abandoned", Some(Failure)),
            ("Later", None),
        ] {
            let s = Scan::of(format!("{RUN}{}", end(word)).as_bytes());
            assert_eq!(s.result().and_then(JobResult::reason), reason, "{word}");
        }
    }

    #[test]
    fn a_forged_success_of_another_word_hides_no_failure() {
        for ok in ["Succeeded", "SucceededWithIssues", "Skipped", "Later"] {
            for log in [format!("{RUN}{}{}", end("Abandoned"), end(ok)), format!("{RUN}{}{}", end(ok), end("Failed"))] {
                let r = Scan::of(log.as_bytes()).result();
                assert!(r.is_some_and(|r| r.reason() == Some(podrunner_schema::ExitReason::Failure)), "{log}");
            }
        }
    }

    #[test]
    fn a_line_of_the_listener_after_a_part_line_of_a_process_still_tells() {
        // A process of the job writes no new line before the real line;
        // then a forged `Succeeded` line. Also after a part longer than
        // the bound of a part of a line.
        for junk in ["x".to_string(), "y ".repeat(3 * LINE_PART)] {
            let log = format!("{RUN}{junk}{}{}", end("Failed"), end("Succeeded"));
            let s = Scan::of(log.as_bytes());
            assert_eq!(s.result(), Some(JobResult::Failed), "{}", junk.len());
        }
        // A job with no completed line: the runner took it.
        let s = Scan::of(RUN.as_bytes());
        assert!(s.running() && s.result().is_none());
    }

    #[test]
    fn chunks_give_the_scan_of_the_whole_log() {
        let log = format!("x\n{HOOK_ENDED}no-answer\n{RUN}{}tail of the log", end("Canceled"));
        for size in [1, 3, 7, 64] {
            let mut s = Scan::default();
            log.as_bytes().chunks(size).for_each(|c| s.feed(c));
            s.end();
            assert_eq!(s.hook.as_deref(), Some("no-answer"), "{size}");
            assert_eq!(s.result(), Some(JobResult::Canceled), "{size}");
            assert_eq!(s.tail(&Marks::default()), log, "{size}");
        }
    }

    #[test]
    fn the_kept_end_of_a_long_log_has_its_bound_and_no_part_of_a_word() {
        let mut s = Scan::default();
        let word = "Q".repeat(40);
        for _ in 0..(3 * KEEP / 41 + 1) {
            s.feed(format!("{word}\n").as_bytes());
        }
        s.feed(b"the end\n");
        s.end();
        assert!(s.tail.len() <= 2 * KEEP);
        let t = s.tail(&Marks::default());
        assert!(t.len() <= LOG_TAIL_MAX && t.ends_with("the end\n"), "{}", t.len());
        // A cut window can start in a word: a whole credential word is
        // masked and a part of one is not, so the first word goes.
        let s = Scan { tail: b"rt-of-a-made-up-secret more\n".to_vec(), tail_cut: true, ..Scan::default() };
        assert_eq!(s.tail(&Marks::default()), " more\n");
    }
}
