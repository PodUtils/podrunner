//! The lines of the end of a unit at a cancel (T-636): the end of a podvm
//! unit can take 15 s or more (its processes end, then its tree goes), and
//! with no line the run looks hung. The run says that it waits for the
//! unit, says each [`TICK`] that it still waits, and says when the unit is
//! gone and how long it took.

use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

/// The time between two lines of a unit that still goes.
pub(super) const TICK: Duration = Duration::from_secs(10);

/// Run `destroy` of the unit `unit` of the job `job`, and give each line
/// to `say` (stderr in the run).
pub(super) fn wait_for<T>(
    job: &str,
    unit: &str,
    tick: Duration,
    say: &(dyn Fn(String) + Sync),
    destroy: impl FnOnce() -> T,
) -> T {
    say(format!(
        "podrunner: {job}: stopping: the run waits for unit {unit} to go (a third signal ends podrunner at once)"
    ));
    let t = Instant::now();
    let (done, ended) = channel::<()>();
    let out = std::thread::scope(|s| {
        s.spawn(move || {
            while let Err(RecvTimeoutError::Timeout) = ended.recv_timeout(tick) {
                say(format!("podrunner: {job}: unit {unit} still goes ({} s)", t.elapsed().as_secs()));
            }
        });
        let out = destroy();
        drop(done);
        out
    });
    say(format!("podrunner: {job}: unit {unit} is gone ({} s)", t.elapsed().as_secs()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// T-636: a slow end of a unit gives a line at its start, a line for
    /// each tick while it goes, and a line at its end. The end waits for
    /// the first tick line (an event), not for a time.
    #[test]
    fn a_slow_end_of_a_unit_says_that_the_run_waits() {
        let lines = Mutex::new(Vec::new());
        let (tx, rx) = channel::<()>();
        let tx = Mutex::new(tx);
        let say = |l: String| {
            if l.contains("still goes") {
                let _ = tx.lock().unwrap().send(());
            }
            lines.lock().unwrap().push(l);
        };
        let got = wait_for("build", "u-1", Duration::from_millis(5), &say, || {
            rx.recv_timeout(Duration::from_secs(30)).expect("a tick line");
            7
        });
        assert_eq!(got, 7);
        let lines = lines.into_inner().unwrap();
        assert_eq!(
            lines.first().map(String::as_str),
            Some(
                "podrunner: build: stopping: the run waits for unit u-1 to go (a third signal ends podrunner at once)"
            )
        );
        assert!(lines[1..lines.len() - 1].iter().all(|l| l.starts_with("podrunner: build: unit u-1 still goes (")));
        assert!(lines.len() >= 3, "{lines:?}");
        assert!(lines.last().is_some_and(|l| l.starts_with("podrunner: build: unit u-1 is gone (")), "{lines:?}");
    }

    /// A fast end gives two lines only.
    #[test]
    fn a_fast_end_of_a_unit_gives_two_lines() {
        let lines = Mutex::new(Vec::new());
        wait_for("a", "u-2", TICK, &|l| lines.lock().unwrap().push(l), || ());
        assert_eq!(lines.into_inner().unwrap().len(), 2);
    }
}
