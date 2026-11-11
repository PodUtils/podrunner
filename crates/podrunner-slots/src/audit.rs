//! The audit lines: one JSON object for each decision about a job, in
//! `STATE_DIR/audit.jsonl`, and forwarded to the source of the job (G-04).
//! A line holds no path, no query, no header and no credential.
//!
//! The lines are a chain (GitHub #59): each has `seq` and `prev`, the
//! SHA-256 of the exact bytes of the line before (its newline left out).
//! [`verify`] finds a lost, cut or edited line; the copy at the source is
//! the anchor of the tail.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use podrunner_schema::{JobId, RefusalCode};
use serde::Serialize;
use serde_json::{Map, Value};

/// The kinds of audit lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Offer,
    Tier,
    Refuse,
    Claim,
    Mint,
    Sign,
    Control,
    HookRefuse,
    /// The runner said that the job completed: the time, no state change.
    JobCompleted,
    JobMismatch,
    PayloadIgnored,
    /// The egress policy of the job allows a host under an open suffix.
    EgressFlagged,
    Destroy,
}

/// One audit line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// The place in the chain, from 1; `Audit::write` sets it.
    pub seq: u64,
    /// The SHA-256 of the line before, in hex; `Audit::write` sets it.
    pub prev: String,
    pub t_ms: u64,
    pub audit: Kind,
    pub source: String,
    pub job_id: JobId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<RefusalCode>,
    /// A short word with no data of the job (a tier, a unit kind).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Entry {
    pub fn to_map(&self) -> Map<String, Value> {
        match serde_json::to_value(self) {
            Ok(Value::Object(m)) => m,
            _ => Map::new(),
        }
    }
}

/// The writer of the audit file.
/// The `prev` of the first line.
pub const ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The lines that stay in memory, for `status` and the tests.
const KEEP: usize = 1000;

/// The SHA-256 of `bytes`, in hex.
pub fn hash(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

pub struct Audit {
    file: File,
    pub lines: Vec<Entry>,
    seq: u64,
    prev: String,
}

/// The `seq` and the hash of the last line of `text` that is a whole JSON
/// object, or (0, ZERO).
fn tail(text: &[u8]) -> (u64, String) {
    for raw in text.split(|b| *b == b'\n').rev() {
        if let Ok(v) = serde_json::from_slice::<Value>(raw) {
            return (v["seq"].as_u64().unwrap_or(0), hash(raw));
        }
    }
    (0, ZERO.into())
}

impl Audit {
    pub fn open(state_dir: &Path) -> std::io::Result<Audit> {
        let path = state_dir.join("audit.jsonl");
        let text = std::fs::read(&path)
            .or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(Vec::new()) } else { Err(e) })?;
        let file = OpenOptions::new().create(true).append(true).mode(0o600).open(&path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        let (seq, prev) = tail(&text);
        let mut a = Audit { file, lines: Vec::new(), seq, prev };
        // A torn last line (a death in a write): end it, and say so in
        // the chain, so that `verify` passes the torn line once.
        if !text.is_empty() && !text.ends_with(b"\n") {
            a.file.write_all(b"\n")?;
            let line = serde_json::json!({ "seq": a.seq + 1, "prev": a.prev, "audit": "resume", "detail": "a torn line before" });
            a.append(&line.to_string())?;
        }
        Ok(a)
    }

    /// Append one line and move the chain on; a failed write moves nothing.
    fn append(&mut self, text: &str) -> std::io::Result<()> {
        self.file.write_all(format!("{text}\n").as_bytes())?;
        self.seq += 1;
        self.prev = hash(text.as_bytes());
        Ok(())
    }

    /// Write `e` as the next line of the chain, and give it back with its
    /// `seq` and `prev` (the copy for the source).
    pub fn write(&mut self, mut e: Entry) -> Entry {
        e.seq = self.seq + 1;
        e.prev = self.prev.clone();
        if let Ok(text) = serde_json::to_string(&e) {
            // An audit line that cannot be written must not stop a job:
            // the journal holds the transitions.
            let _ = self.append(&text);
        }
        self.lines.push(e.clone());
        if self.lines.len() > KEEP {
            self.lines.drain(..self.lines.len() - KEEP);
        }
        e
    }
}

/// Walk the chain of `text`. `anchor` is `(seq, hash)`: the hash of line
/// `seq`, as the `prev` of the copy of line `seq + 1` at the source says.
/// The result is the last `seq`, or the place where the chain breaks.
pub fn verify(text: &[u8], anchor: Option<(u64, &str)>) -> Result<u64, String> {
    if !text.is_empty() && !text.ends_with(b"\n") {
        return Err("a torn line at the end".into());
    }
    let (mut prev, mut next, mut torn) = (ZERO.to_string(), 1u64, false);
    let mut anchored = anchor.is_none();
    for raw in text.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        let Ok(v) = serde_json::from_slice::<Value>(raw) else {
            torn = true;
            continue;
        };
        let seq = v["seq"].as_u64().ok_or("a line with no seq")?;
        if torn && v["audit"] != "resume" {
            return Err(format!("a torn line before seq {seq}"));
        }
        if seq != next || v["prev"].as_str() != Some(prev.as_str()) {
            return Err(format!("broken at seq {seq}"));
        }
        prev = hash(raw);
        if let Some((aseq, ahash)) = anchor
            && aseq == seq
        {
            if prev != ahash {
                return Err(format!("broken at seq {seq}: the anchor differs"));
            }
            anchored = true;
        }
        (next, torn) = (seq + 1, false);
    }
    if torn {
        return Err("a torn line at the end".into());
    }
    if !anchored {
        return Err(format!("cut: no line {} of the anchor", anchor.map(|a| a.0).unwrap_or(0)));
    }
    Ok(next - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("podrunner-audit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn entry(n: u64, detail: &str) -> Entry {
        Entry {
            seq: 0,
            prev: String::new(),
            t_ms: n,
            audit: Kind::Offer,
            source: "s".into(),
            job_id: JobId::new(&format!("j-{n}")).unwrap(),
            code: None,
            detail: Some(detail.into()),
        }
    }

    /// A file of five lines.
    fn five(d: &Path) -> Vec<u8> {
        let mut a = Audit::open(d).unwrap();
        for n in 1..=5 {
            a.write(entry(n, &format!("d{n}")));
        }
        std::fs::read(d.join("audit.jsonl")).unwrap()
    }

    fn lines(text: &[u8]) -> Vec<Vec<u8>> {
        text.split(|b| *b == b'\n').filter(|l| !l.is_empty()).map(<[u8]>::to_vec).collect()
    }

    fn join(l: &[Vec<u8>]) -> Vec<u8> {
        l.iter().flat_map(|x| x.iter().copied().chain(*b"\n")).collect()
    }

    #[test]
    fn a_chain_verifies_and_each_break_is_found() {
        let d = dir("chain");
        let text = five(&d);
        assert_eq!(verify(&text, None), Ok(5));
        let l = lines(&text);
        // A lost line.
        let mut lost = l.clone();
        lost.remove(2);
        assert_eq!(verify(&join(&lost), None), Err("broken at seq 4".into()));
        // One edited byte in a detail.
        let edited = join(&l);
        let s = String::from_utf8(edited).unwrap().replacen("\"d2\"", "\"x2\"", 1);
        assert_eq!(verify(s.as_bytes(), None), Err("broken at seq 3".into()));
        // A cut tail: only the anchor of the source finds it.
        let cut = join(&l[..3]);
        assert_eq!(verify(&cut, None), Ok(3));
        let v: Value = serde_json::from_slice(&l[4]).unwrap();
        let anchor = v["prev"].as_str().unwrap().to_string();
        assert_eq!(verify(&text, Some((4, &anchor))), Ok(5));
        assert!(verify(&cut, Some((4, &anchor))).is_err());
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(d.join("audit.jsonl")).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_torn_line_fails_until_the_resume_line() {
        let d = dir("torn");
        let mut text = five(&d);
        text.extend_from_slice(b"{\"seq\":6,\"pr");
        std::fs::write(d.join("audit.jsonl"), &text).unwrap();
        assert_eq!(verify(&text, None), Err("a torn line at the end".into()));
        let mut a = Audit::open(&d).unwrap();
        a.write(entry(7, "after"));
        let text = std::fs::read(d.join("audit.jsonl")).unwrap();
        assert_eq!(verify(&text, None), Ok(7));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_failed_write_moves_no_chain_and_memory_is_bounded() {
        let d = dir("full");
        let mut a = Audit::open(&d).unwrap();
        a.write(entry(1, "a"));
        a.file = OpenOptions::new().write(true).open("/dev/full").unwrap();
        a.write(entry(2, "lost"));
        a.file = OpenOptions::new().append(true).open(d.join("audit.jsonl")).unwrap();
        a.write(entry(3, "b"));
        assert_eq!(verify(&std::fs::read(d.join("audit.jsonl")).unwrap(), None), Ok(2));
        for n in 0..5000 {
            a.write(entry(n, "x"));
        }
        assert_eq!(a.lines.len(), KEEP);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_change_in_a_detail_changes_the_next_prev() {
        let (a, b) = (dir("a"), dir("b"));
        for (d, detail) in [(&a, "x|y"), (&b, "x||y")] {
            let mut au = Audit::open(d).unwrap();
            au.write(entry(1, detail));
            au.write(entry(2, "z"));
        }
        let prev = |d: &Path| {
            let t = std::fs::read(d.join("audit.jsonl")).unwrap();
            serde_json::from_slice::<Value>(&lines(&t)[1]).unwrap()["prev"].clone()
        };
        assert_ne!(prev(&a), prev(&b));
        let _ = (std::fs::remove_dir_all(&a), std::fs::remove_dir_all(&b));
    }
}
