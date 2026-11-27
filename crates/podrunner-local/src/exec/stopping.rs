//! The lines of the end of a unit at a cancel (T-636): the end of a podvm
//! unit can take 15 s or more (its processes end, then its tree goes), and
//! with no line the run looks hung. The run says that it waits for the
//! unit, says each [`TICK`] that it still waits, and says when the unit is
//! gone and how long it took. A unit that does not go in its bound is
//! handed to the helper of a third signal (T-716).

use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

/// The time between two lines of a unit that still goes.
pub(super) const TICK: Duration = Duration::from_secs(10);

/// The pace of the wait: a line each `tick`, and `overdue` once at
/// `bound` while the unit still goes. With no bound, the wait has no end.
pub(super) struct Pace<'a> {
    pub tick: Duration,
    pub bound: Option<Duration>,
    pub overdue: &'a (dyn Fn(String) + Sync),
}

/// Run `destroy` of the unit `unit` of the job `job`, and give each line
/// to `say` (stderr in the run). At the bound of `pace`, give the line of
/// the hand-over to `pace.overdue`: in the run it ends the process and a
/// helper ends the unit ([`crate::cancel::end_now`]).
pub(super) fn wait_for<T>(
    job: &str,
    unit: &str,
    pace: Pace,
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
            let mut next = t + pace.tick;
            let mut bound = pace.bound.map(|b| t + b);
            loop {
                let at = bound.map_or(next, |b| b.min(next));
                match ended.recv_timeout(at.saturating_duration_since(Instant::now())) {
                    Err(RecvTimeoutError::Timeout) => {}
                    _ => return,
                }
                if bound.is_some_and(|b| Instant::now() >= b) {
                    bound = None;
                    (pace.overdue)(format!(
                        "podrunner: {job}: unit {unit} did not go in {} s; podrunner ends now, and a helper ends the unit",
                        t.elapsed().as_secs()
                    ));
                }
                if Instant::now() >= next {
                    next += pace.tick;
                    say(format!("podrunner: {job}: unit {unit} still goes ({} s)", t.elapsed().as_secs()));
                }
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
        let pace = Pace { tick: Duration::from_millis(5), bound: None, overdue: &|l| panic!("{l}") };
        let got = wait_for("build", "u-1", pace, &say, || {
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
        let pace = Pace { tick: TICK, bound: Some(TICK), overdue: &|l| panic!("{l}") };
        wait_for("a", "u-2", pace, &|l| lines.lock().unwrap().push(l), || ());
        assert_eq!(lines.into_inner().unwrap().len(), 2);
    }

    /// T-716: a unit that does not go in the bound gives the line of the
    /// hand-over to `overdue` once; in the run, `overdue` ends the process.
    /// Here the end of the unit waits for that line (an event).
    #[test]
    fn a_unit_that_does_not_go_is_handed_over_at_the_bound() {
        let lines = Mutex::new(Vec::new());
        let calls = Mutex::new(Vec::new());
        let (tx, rx) = channel::<()>();
        let tx = Mutex::new(tx);
        let overdue = |l: String| {
            calls.lock().unwrap().push(l);
            let _ = tx.lock().unwrap().send(());
        };
        let pace = Pace { tick: TICK, bound: Some(Duration::from_millis(20)), overdue: &overdue };
        let t = Instant::now();
        wait_for("build", "u-3", pace, &|l| lines.lock().unwrap().push(l), || {
            rx.recv_timeout(Duration::from_secs(30)).expect("the hand-over");
        });
        assert!(t.elapsed() >= Duration::from_millis(20), "{:?}", t.elapsed());
        let calls = calls.into_inner().unwrap();
        assert_eq!(calls.len(), 1, "{calls:?}");
        assert!(
            calls[0].starts_with("podrunner: build: unit u-3 did not go in ")
                && calls[0].ends_with(" s; podrunner ends now, and a helper ends the unit"),
            "{calls:?}"
        );
        assert_eq!(lines.into_inner().unwrap().len(), 2, "no tick line before the bound");
    }

    /// T-716: the bound also holds when it is past the first tick; the
    /// tick lines go on until the bound.
    #[test]
    fn the_bound_comes_after_the_ticks() {
        let ticks = Mutex::new(0);
        let (tx, rx) = channel::<()>();
        let tx = Mutex::new(tx);
        let say = |l: String| {
            if l.contains("still goes") {
                *ticks.lock().unwrap() += 1;
            }
        };
        let overdue = |_| {
            let _ = tx.lock().unwrap().send(());
        };
        let pace = Pace { tick: Duration::from_millis(5), bound: Some(Duration::from_millis(60)), overdue: &overdue };
        let t = Instant::now();
        wait_for("b", "u-4", pace, &say, || rx.recv_timeout(Duration::from_secs(30)).expect("the hand-over"));
        assert!(t.elapsed() >= Duration::from_millis(60), "{:?}", t.elapsed());
        assert!(*ticks.lock().unwrap() >= 1, "no tick line before the bound");
    }
}
