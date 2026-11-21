//! The log of a unit as a stream, and a read of it with a bound of bytes
//! and of time (T-570). The core reads the log of each runner job that
//! exits 0, in its tick; a job sets the size of its log, so a read with
//! no bound can hold the tick and the memory. A writer process that takes
//! a bound (`podvm logs --limit-bytes N`) gets the bound of the read too,
//! so it stops by itself.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::{UnitCode, UnitError};

/// The output of the payload of a unit, from its start: a reader, and the
/// process that writes it when one does (`podvm logs`).
pub struct UnitLog(Source);

/// A command of a writer for a bound of bytes (`None`: all of the log).
type Spawn = Box<dyn FnOnce(Option<u64>) -> Command + Send>;

enum Source {
    Open(Stream),
    /// The writer starts at the read, with the bound of the read.
    Later(Spawn),
}

struct Stream {
    reader: Box<dyn Read + Send>,
    writer: Option<Writer>,
}

/// A writer process, and the last bytes of its stderr: they tell why it
/// failed (podvm names a refusal there).
struct Writer {
    child: Child,
    stderr: Option<mpsc::Receiver<Vec<u8>>>,
}

/// Why a bounded read stopped before the end of the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cut {
    /// The read got the bound of bytes.
    Bytes,
    /// The read got the bound of time.
    Time,
}

/// The result of a bounded read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogRead {
    /// The bytes that the read gave to its sink.
    pub bytes: u64,
    /// `None`: the read got the end of the log.
    pub cut: Option<Cut>,
}

/// The size of one chunk of a read.
const CHUNK: usize = 64 * 1024;

/// The most bytes of the stderr of a writer that an error keeps.
const STDERR_KEEP: usize = 2048;

/// The most time to wait for the stderr of a writer that failed.
const STDERR_WAIT: Duration = Duration::from_millis(500);

fn host(what: &str, e: impl std::fmt::Display) -> UnitError {
    UnitError::new(UnitCode::Host, format!("the log of the unit: {what}: {e}"))
}

impl UnitLog {
    /// A log that the driver holds in memory.
    pub fn bytes(log: Vec<u8>) -> UnitLog {
        UnitLog(Source::Open(Stream { reader: Box::new(std::io::Cursor::new(log)), writer: None }))
    }

    /// A log in a file.
    pub fn file(path: &std::path::Path) -> Result<UnitLog, UnitError> {
        let f = std::fs::File::open(path).map_err(|e| host("open", e.kind()))?;
        Ok(UnitLog(Source::Open(Stream { reader: Box::new(f), writer: None })))
    }

    /// A log on the stdout of `cmd`. Its stderr is not part of the log. A
    /// code other than 0 makes the read fail, with the last line of stderr.
    pub fn command(cmd: Command) -> Result<UnitLog, UnitError> {
        Ok(UnitLog(Source::Open(spawn(cmd)?)))
    }

    /// A log on the stdout of the command that `spawn` gives for a bound
    /// of bytes: `read_bounded` gives one byte more than its bound, so the
    /// read still tells a cut from a log of exactly the bound; `read_all`
    /// gives `None`. The writer starts at the read.
    pub fn limited(spawn: impl FnOnce(Option<u64>) -> Command + Send + 'static) -> UnitLog {
        UnitLog(Source::Later(Box::new(spawn)))
    }

    fn open(self, bound: Option<u64>) -> Result<Stream, UnitError> {
        match self.0 {
            Source::Open(s) => Ok(s),
            Source::Later(f) => spawn(f(bound)),
        }
    }

    /// All of the log, with no bound: for a test or a person.
    pub fn read_all(self) -> Result<Vec<u8>, UnitError> {
        let Stream { mut reader, writer } = self.open(None)?;
        let mut out = Vec::new();
        let read = reader.read_to_end(&mut out);
        drop(reader);
        end(writer, read.is_err(), None)?;
        read.map_err(|e| host("read", e.kind()))?;
        Ok(out)
    }

    /// Give at most `max` bytes of the log to `sink`, in at most `time`.
    /// A thread reads, so a writer that blocks does not hold the caller.
    /// At a cut the writer process is killed: its code does not count (a
    /// closed stdout ends `podvm logs` with 141).
    pub fn read_bounded(self, max: u64, time: Duration, sink: &mut dyn FnMut(&[u8])) -> Result<LogRead, UnitError> {
        let deadline = Instant::now() + time;
        let Stream { mut reader, writer: child } = self.open(Some(max.saturating_add(1)))?;
        // Two chunks in the channel at most: the memory has a bound too.
        let (tx, rx) = mpsc::sync_channel::<std::io::Result<Vec<u8>>>(2);
        std::thread::spawn(move || {
            let mut buf = vec![0u8; CHUNK];
            loop {
                let r = match reader.read(&mut buf) {
                    Ok(0) => return,
                    Ok(n) => Ok(buf[..n].to_vec()),
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => Err(e),
                };
                let failed = r.is_err();
                if tx.send(r).is_err() || failed {
                    return;
                }
            }
        });
        let mut done = LogRead { bytes: 0, cut: None };
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(Ok(chunk)) => {
                    let room = usize::try_from(max - done.bytes).unwrap_or(usize::MAX);
                    let take = chunk.len().min(room);
                    sink(&chunk[..take]);
                    done.bytes += take as u64;
                    // At the bound, the next chunk is the cut: a log of
                    // exactly `max` bytes is whole.
                    if take < chunk.len() {
                        done.cut = Some(Cut::Bytes);
                        break;
                    }
                }
                Ok(Err(e)) => {
                    let _ = end(child, true, None);
                    return Err(host("read", e.kind()));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    done.cut = Some(Cut::Time);
                    break;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        drop(rx);
        end(child, done.cut.is_some(), Some(deadline))?;
        Ok(done)
    }
}

/// Start `cmd` as the writer of a log: its stdout is the log, and a
/// thread keeps the last bytes of its stderr.
fn spawn(mut cmd: Command) -> Result<Stream, UnitError> {
    let mut child =
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| host("spawn", e))?;
    let out = child.stdout.take().ok_or_else(|| host("spawn", "no stdout"))?;
    let stderr = child.stderr.take().map(|mut err| {
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let (mut kept, mut buf) = (Vec::new(), [0u8; 4096]);
            while let Ok(n @ 1..) = err.read(&mut buf) {
                kept.extend_from_slice(&buf[..n]);
                let over = kept.len().saturating_sub(STDERR_KEEP);
                kept.drain(..over);
            }
            let _ = tx.send(kept);
        });
        rx
    });
    Ok(Stream { reader: Box::new(out), writer: Some(Writer { child, stderr }) })
}

/// The last line of the stderr of a writer that ended, or an empty text.
fn last_words(stderr: Option<mpsc::Receiver<Vec<u8>>>) -> String {
    let Some(kept) = stderr.and_then(|rx| rx.recv_timeout(STDERR_WAIT).ok()) else { return String::new() };
    let text = String::from_utf8_lossy(&kept);
    text.lines().rev().find(|l| !l.trim().is_empty()).map(|l| format!(": {}", l.trim())).unwrap_or_default()
}

/// End the writer process of a log: kill it after a cut or at `deadline`,
/// and reap it. A code other than 0 after a whole read is an error.
fn end(writer: Option<Writer>, cut: bool, deadline: Option<Instant>) -> Result<(), UnitError> {
    let Some(Writer { mut child, stderr }) = writer else { return Ok(()) };
    let status = match (cut, deadline) {
        (true, _) => None,
        (false, None) => child.wait().ok(),
        (false, Some(d)) => loop {
            match child.try_wait() {
                Ok(None) if Instant::now() < d => std::thread::sleep(Duration::from_millis(5)),
                Ok(s) => break s,
                Err(_) => break None,
            }
        },
    };
    match status {
        Some(s) if s.success() => Ok(()),
        Some(s) => Err(host("the writer", format!("{s}{}", last_words(stderr)))),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            // A writer that did not end in time gave its output: the read
            // tells the cut.
            if cut { Ok(()) } else { Err(host("the writer", "no end in time")) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(script: &str) -> UnitLog {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        UnitLog::command(c).unwrap()
    }

    fn read(log: UnitLog, max: u64, time: Duration) -> (Result<LogRead, UnitError>, Vec<u8>) {
        let mut got = Vec::new();
        let r = log.read_bounded(max, time, &mut |b| got.extend_from_slice(b));
        (r, got)
    }

    #[test]
    fn a_read_gives_at_most_the_bound_of_bytes() {
        let (r, got) = read(UnitLog::bytes(vec![b'x'; 300_000]), 100_000, Duration::from_secs(5));
        assert_eq!(r.unwrap(), LogRead { bytes: 100_000, cut: Some(Cut::Bytes) });
        assert_eq!(got.len(), 100_000);
        // A log of exactly the bound is whole.
        let (r, _) = read(UnitLog::bytes(vec![b'x'; 100_000]), 100_000, Duration::from_secs(5));
        assert_eq!(r.unwrap(), LogRead { bytes: 100_000, cut: None });
        // A writer process: it is killed at the cut.
        let (r, got) = read(sh("yes x"), 1000, Duration::from_secs(5));
        assert_eq!(r.unwrap().cut, Some(Cut::Bytes));
        assert_eq!(got, b"x\n".repeat(500));
    }

    #[test]
    fn a_writer_that_blocks_holds_the_read_no_longer_than_its_bound_of_time() {
        let t0 = Instant::now();
        let (r, got) = read(sh("echo start; exec sleep 30"), 1000, Duration::from_millis(300));
        assert_eq!(r.unwrap(), LogRead { bytes: 6, cut: Some(Cut::Time) });
        assert_eq!(got, b"start\n");
        assert!(t0.elapsed() < Duration::from_secs(5), "{:?}", t0.elapsed());
    }

    #[test]
    fn a_writer_that_fails_gives_no_log() {
        assert!(read(sh("echo some; exit 3"), 1000, Duration::from_secs(5)).0.is_err());
        assert!(sh("echo some; exit 3").read_all().is_err());
        assert_eq!(sh("echo some").read_all().unwrap(), b"some\n");
    }

    /// The error of a writer that failed names its reason: podvm's incus
    /// tier refuses a log over 64 MiB with a line on stderr.
    #[test]
    fn the_error_of_a_writer_names_the_last_line_of_its_stderr() {
        let script = "echo some; echo 'podvm logs: u-1: the answer is over 67108864 bytes' >&2; exit 1";
        let e = read(sh(script), 1000, Duration::from_secs(5)).0.unwrap_err();
        assert!(e.detail.contains("over 67108864 bytes"), "{e}");
        assert!(sh(script).read_all().unwrap_err().detail.contains("over 67108864 bytes"));
    }

    /// A writer with a bound: `podvm logs --limit-bytes N` stops after N
    /// bytes with exit 0. The read gives it one byte past its own bound.
    fn limited(seen: std::sync::Arc<std::sync::Mutex<Vec<Option<u64>>>>) -> UnitLog {
        UnitLog::limited(move |bound| {
            seen.lock().unwrap().push(bound);
            let mut c = Command::new("sh");
            match bound {
                Some(n) => c.args(["-c", &format!("yes x | head -c {n}")]),
                None => c.args(["-c", "yes x | head -c 3000"]),
            };
            c
        })
    }

    #[test]
    fn a_writer_with_a_bound_gets_one_byte_past_the_bound_of_the_read() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (r, got) = read(limited(seen.clone()), 1000, Duration::from_secs(5));
        assert_eq!(r.unwrap(), LogRead { bytes: 1000, cut: Some(Cut::Bytes) });
        assert_eq!(got, b"x\n".repeat(500));
        // The writer gives 3001 bytes for a bound of 3000: the read cuts.
        let (r, _) = read(limited(seen.clone()), 3000, Duration::from_secs(5));
        assert_eq!(r.unwrap(), LogRead { bytes: 3000, cut: Some(Cut::Bytes) });
        assert_eq!(limited(seen.clone()).read_all().unwrap().len(), 3000);
        assert_eq!(*seen.lock().unwrap(), [Some(1001), Some(3001), None]);
    }

    /// At a cut, a closed stdout ends the writer by SIGPIPE (podvm gives
    /// 141): no failure. A whole read keeps the code of the writer.
    #[test]
    fn a_writer_that_a_cut_ends_with_141_is_no_failure() {
        let (r, got) = read(sh("yes x | head -c 5000; exit 141"), 1000, Duration::from_secs(5));
        assert_eq!(r.unwrap(), LogRead { bytes: 1000, cut: Some(Cut::Bytes) });
        assert_eq!(got.len(), 1000);
        assert!(read(sh("echo some; exit 141"), 1000, Duration::from_secs(5)).0.is_err());
    }
}
