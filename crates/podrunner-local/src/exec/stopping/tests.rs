use super::*;
use std::sync::mpsc::{Receiver, Sender};

/// The limit of each wait for an event.
const LIMIT: Duration = Duration::from_secs(30);

/// A sink of lines: the lines, and a send of each line that contains `mark`.
fn lines(mark: &'static str) -> (Arc<Mutex<Vec<String>>>, Say, Receiver<String>) {
    let all = Arc::new(Mutex::new(Vec::new()));
    let (tx, rx) = channel::<String>();
    let tx = Mutex::new(tx);
    let keep = all.clone();
    let say: Say = Arc::new(move |l: String| {
        if l.contains(mark) {
            let _ = tx.lock().unwrap().send(l.clone());
        }
        keep.lock().unwrap().push(l);
    });
    (all, say, rx)
}

/// An overdue that sends its line, and returns: the run goes on.
fn overdue_to(tx: Sender<String>) -> impl Fn(String) + Sync {
    let tx = Mutex::new(tx);
    move |l| {
        let _ = tx.lock().unwrap().send(l);
    }
}

/// T-636: a slow end of a unit gives a line at its start, a line for
/// each tick while it goes, and a line at its end. The end waits for
/// the first tick line (an event), not for a time.
#[test]
fn a_slow_end_of_a_unit_says_that_the_run_waits() {
    let (all, say, ticks) = lines("still goes");
    let pace = Pace { tick: Duration::from_millis(5), bound: None, overdue: &|l| panic!("{l}") };
    let got = wait_for("build", "u-1", pace, &say, move || {
        ticks.recv_timeout(LIMIT).expect("a tick line");
        7
    });
    assert_eq!(got, Some(7));
    let lines = all.lock().unwrap().clone();
    assert_eq!(
        lines.first().map(String::as_str),
        Some("podrunner: build: stopping: the run waits for unit u-1 to go (a third signal ends podrunner at once)")
    );
    assert!(lines[1..lines.len() - 1].iter().all(|l| l.starts_with("podrunner: build: unit u-1 still goes (")));
    assert!(lines.len() >= 3, "{lines:?}");
    assert!(lines.last().is_some_and(|l| l.starts_with("podrunner: build: unit u-1 is gone (")), "{lines:?}");
}

/// A fast end gives two lines only.
#[test]
fn a_fast_end_of_a_unit_gives_two_lines() {
    let (all, say, _) = lines("-");
    let pace = Pace { tick: TICK, bound: Some(TICK), overdue: &|l| panic!("{l}") };
    assert_eq!(wait_for("a", "u-2", pace, &say, || ()), Some(()));
    assert_eq!(all.lock().unwrap().len(), 2);
}

/// T-716, T-720: a unit that does not go in the bound gives the line of
/// the bound to `overdue` once. When `overdue` returns (no signal), the
/// wait ends with no answer, [`going`] counts the unit, and the end of the
/// unit goes on: its line comes when it is gone. Each step waits for an
/// event.
#[test]
fn a_unit_that_does_not_go_goes_on_in_the_background_after_the_bound() {
    let (all, say, gone) = lines("is gone");
    let (tx, at_bound) = channel::<String>();
    let overdue = overdue_to(tx);
    let (release, released) = channel::<()>();
    let pace = Pace { tick: TICK, bound: Some(Duration::from_millis(20)), overdue: &overdue };
    let t = Instant::now();
    let got = wait_for("build", "u-3", pace, &say, move || released.recv_timeout(LIMIT).expect("the release"));
    assert_eq!(got, None, "the wait ended at the bound");
    assert!(t.elapsed() >= Duration::from_millis(20), "{:?}", t.elapsed());
    let line = at_bound.try_recv().expect("the line of the bound");
    assert!(line.starts_with("podrunner: build: unit u-3 did not go in ") && line.ends_with(" s"), "{line}");
    assert!(at_bound.try_recv().is_err(), "one line of the bound");
    assert!(going() >= 1, "the unit goes in the background");
    assert_eq!(all.lock().unwrap().len(), 1, "no tick line and no end line before the bound");
    release.send(()).unwrap();
    let line = gone.recv_timeout(LIMIT).expect("the end of the unit");
    assert!(line.starts_with("podrunner: build: unit u-3 is gone ("), "{line}");
}

/// T-716: the bound also holds when it is past the first tick; the
/// tick lines go on until the bound.
#[test]
fn the_bound_comes_after_the_ticks() {
    let (all, say, _) = lines("-");
    let (tx, at_bound) = channel::<String>();
    let overdue = overdue_to(tx);
    let (release, released) = channel::<()>();
    let pace = Pace { tick: Duration::from_millis(5), bound: Some(Duration::from_millis(60)), overdue: &overdue };
    let t = Instant::now();
    let got = wait_for("b", "u-4", pace, &say, move || released.recv_timeout(LIMIT).expect("the release"));
    assert_eq!(got, None);
    assert!(t.elapsed() >= Duration::from_millis(60), "{:?}", t.elapsed());
    assert!(at_bound.try_recv().is_ok(), "the line of the bound");
    assert!(all.lock().unwrap().iter().any(|l| l.contains("still goes")), "no tick line before the bound");
    release.send(()).unwrap();
}

/// T-720: a directory that the run gives while a unit goes in the
/// background stays until the unit is gone. Each step waits for an
/// event.
#[test]
fn the_files_of_a_run_stay_until_its_units_are_gone() {
    let base = std::env::temp_dir().join(format!("podrunner-local-going-{}", std::process::id()));
    let (now, later) = (base.join("now"), base.join("later"));
    std::fs::create_dir_all(&now).unwrap();
    std::fs::create_dir_all(&later).unwrap();
    let (_, say, gone) = lines("is gone");
    let (tx, _at_bound) = channel::<String>();
    let overdue = overdue_to(tx);
    let (release, released) = channel::<()>();
    let pace = Pace { tick: TICK, bound: Some(Duration::ZERO), overdue: &overdue };
    let got = wait_for("c", "u-5", pace, &say, move || released.recv_timeout(LIMIT).expect("the release"));
    assert_eq!(got, None);
    remove_when_gone(vec![later.clone()]);
    assert!(later.is_dir(), "a unit still goes: the directory stays");
    release.send(()).unwrap();
    gone.recv_timeout(LIMIT).expect("the end of the unit");
    // Another test can hold a unit of its own a short time longer.
    let gone_in_limit = |d: &PathBuf| {
        let t = Instant::now();
        while d.exists() && t.elapsed() < LIMIT {
            std::thread::sleep(Duration::from_millis(10));
        }
        !d.exists()
    };
    assert!(gone_in_limit(&later), "the directory went with the last unit");
    remove_when_gone(vec![now.clone()]);
    assert!(gone_in_limit(&now), "the directory went");
    let _ = std::fs::remove_dir_all(&base);
}
