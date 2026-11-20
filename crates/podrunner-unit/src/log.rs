//! The log of a unit as a stream, and a read of it with a bound of bytes
//! and of time (T-570). The core reads the log of each runner job that
//! exits 0, in its tick; a job sets the size of its log, so a read with
//! no bound can hold the tick and the memory.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::{UnitCode, UnitError};

/// The output of the payload of a unit, from its start: a reader, and the
/// process that writes it when one does (`podvm logs`).
pub struct UnitLog {
    reader: Box<dyn Read + Send>,
    child: Option<Child>,
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

fn host(what: &str, e: impl std::fmt::Display) -> UnitError {
    UnitError::new(UnitCode::Host, format!("the log of the unit: {what}: {e}"))
}

impl UnitLog {
    /// A log that the driver holds in memory.
    pub fn bytes(log: Vec<u8>) -> UnitLog {
        UnitLog { reader: Box::new(std::io::Cursor::new(log)), child: None }
    }

    /// A log in a file.
    pub fn file(path: &std::path::Path) -> Result<UnitLog, UnitError> {
        let f = std::fs::File::open(path).map_err(|e| host("open", e.kind()))?;
        Ok(UnitLog { reader: Box::new(f), child: None })
    }

    /// A log on the stdout of `cmd`. Its stderr is not part of the log. A
    /// code other than 0 makes the read fail.
    pub fn command(mut cmd: Command) -> Result<UnitLog, UnitError> {
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| host("spawn", e))?;
        let out = child.stdout.take().ok_or_else(|| host("spawn", "no stdout"))?;
        Ok(UnitLog { reader: Box::new(out), child: Some(child) })
    }

    /// All of the log, with no bound: for a test or a person.
    pub fn read_all(self) -> Result<Vec<u8>, UnitError> {
        let UnitLog { mut reader, child } = self;
        let mut out = Vec::new();
        let read = reader.read_to_end(&mut out);
        drop(reader);
        end(child, read.is_err(), None)?;
        read.map_err(|e| host("read", e.kind()))?;
        Ok(out)
    }

    /// Give at most `max` bytes of the log to `sink`, in at most `time`.
    /// A thread reads, so a writer that blocks does not hold the caller.
    /// At a cut the writer process is killed.
    pub fn read_bounded(self, max: u64, time: Duration, sink: &mut dyn FnMut(&[u8])) -> Result<LogRead, UnitError> {
        let deadline = Instant::now() + time;
        let UnitLog { mut reader, child } = self;
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

/// End the writer process of a log: kill it after a cut or at `deadline`,
/// and reap it. A code other than 0 after a whole read is an error.
fn end(child: Option<Child>, cut: bool, deadline: Option<Instant>) -> Result<(), UnitError> {
    let Some(mut child) = child else { return Ok(()) };
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
        Some(s) => Err(host("the writer", s)),
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
}
