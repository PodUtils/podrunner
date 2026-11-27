//! The end of a unit at a cancel (T-636): the end of a podvm unit can take
//! 15 s or more (its processes end, then its tree goes), and with no line
//! the run looks hung. The run says that it waits for the unit, says each
//! [`TICK`] that it still waits, and says when the unit is gone and how
//! long it took. Each destroy at a cancel has these lines and a bound
//! (T-721). At the bound, after a signal, the helper of a third signal
//! ends the unit (T-716); after `fail-fast` or a change under `--watch`,
//! the end of the unit goes on in the background and the run goes on
//! (T-720).

use std::path::PathBuf;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::hooks::Hooks;
use podrunner_unit::Residue;

/// The time between two lines of a unit that still goes.
pub(super) const TICK: Duration = Duration::from_secs(10);

/// The sink of the lines (stderr in the run).
pub(super) type Say = Arc<dyn Fn(String) + Send + Sync>;

/// The pace of the wait: a line each `tick`, and `overdue` once at
/// `bound` while the unit still goes. With no bound, the wait has no end.
pub(super) struct Pace<'a> {
    pub tick: Duration,
    pub bound: Option<Duration>,
    pub overdue: &'a (dyn Fn(String) + Sync),
}

/// The units whose end goes on in the background, and the directories to
/// remove when the last of them is gone.
struct Going {
    units: usize,
    dirs: Vec<PathBuf>,
}

static GOING: Mutex<Going> = Mutex::new(Going { units: 0, dirs: Vec::new() });

fn going_now() -> MutexGuard<'static, Going> {
    GOING.lock().unwrap_or_else(|e| e.into_inner())
}

/// The count of the units whose end goes on in the background (T-720).
pub fn going() -> usize {
    going_now().units
}

/// Remove `dirs` now, or, while a unit goes in the background, when the
/// last one is gone: a unit can use them until it is gone (T-720).
pub fn remove_when_gone(dirs: Vec<PathBuf>) {
    let mut g = going_now();
    match g.units {
        0 => {
            drop(g);
            remove(dirs);
        }
        _ => g.dirs.extend(dirs),
    }
}

fn remove(dirs: Vec<PathBuf>) {
    for d in dirs {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// The state of one destroy: it ended, or the wait for it ended at the bound.
#[derive(Default)]
struct Ends {
    done: bool,
    late: bool,
}

/// Run `destroy` of the unit `unit` of the job `job`, and give each line
/// to `say`. At the bound of `pace`, give the line of the bound to
/// `pace.overdue`: after a signal it ends the process and a helper ends
/// the unit ([`crate::cancel::end_now`]). When `overdue` returns, the
/// destroy goes on in the background, [`going`] counts it until it ends,
/// and the answer is `None`.
pub(super) fn wait_for<T: Send + 'static>(
    job: &str,
    unit: &str,
    pace: Pace,
    say: &Say,
    destroy: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    say(format!(
        "podrunner: {job}: stopping: the run waits for unit {unit} to go (a third signal ends podrunner at once)"
    ));
    let t = Instant::now();
    let gone = {
        let (job, unit) = (job.to_string(), unit.to_string());
        move || format!("podrunner: {job}: unit {unit} is gone ({} s)", t.elapsed().as_secs())
    };
    let ends = Arc::new(Mutex::new(Ends::default()));
    let (tx, rx) = channel::<T>();
    let worker = {
        let (ends, say, gone) = (ends.clone(), say.clone(), gone.clone());
        std::thread::spawn(move || {
            let out = destroy();
            let mut e = ends.lock().unwrap_or_else(|e| e.into_inner());
            e.done = true;
            let late = e.late;
            if !late {
                let _ = tx.send(out);
            }
            drop(e);
            if late {
                say(gone());
                let mut g = going_now();
                g.units -= 1;
                let dirs = if g.units == 0 { std::mem::take(&mut g.dirs) } else { Vec::new() };
                drop(g);
                remove(dirs);
            }
        })
    };
    let mut next = t + pace.tick;
    let mut bound = pace.bound.map(|b| t + b);
    loop {
        let at = bound.map_or(next, |b| b.min(next));
        match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
            Ok(out) => {
                say(gone());
                return Some(out);
            }
            // The destroy panicked: the panic goes on here, as on the
            // thread of the job.
            Err(RecvTimeoutError::Disconnected) => match worker.join() {
                Err(p) => std::panic::resume_unwind(p),
                Ok(()) => unreachable!("the destroy ended with no answer"),
            },
            Err(RecvTimeoutError::Timeout) => {}
        }
        if bound.is_some_and(|b| Instant::now() >= b) {
            bound = None;
            let mut e = ends.lock().unwrap_or_else(|e| e.into_inner());
            if !e.done {
                e.late = true;
                going_now().units += 1;
                drop(e);
                (pace.overdue)(format!("podrunner: {job}: unit {unit} did not go in {} s", t.elapsed().as_secs()));
                return None;
            }
        }
        if Instant::now() >= next {
            next += pace.tick;
            say(format!("podrunner: {job}: unit {unit} still goes ({} s)", t.elapsed().as_secs()));
        }
    }
}

/// The end of the unit of a job: what the destroy left, `None` when its
/// end goes on in the background (T-720); and the lines of the hooks.
pub(super) type Ended = (Option<Result<Residue, String>>, Vec<String>);

/// End the unit `unit` of the job `job` with `destroy`, and stop its
/// `hooks`. A job that did not end (`open`) can hold a hook call on its
/// container step until the container ends: the unit goes first, so the
/// stop of the hooks waits for no step (T-639). A hook call that was in
/// `podvm run -d` at the first destroy can start a container after it:
/// the unit is destroyed again (it is idempotent). At a cancel
/// (`stopping`), each destroy goes through [`wait_for`] with `bound`
/// (T-721).
pub(super) fn end_unit(
    (job, unit): (&str, &str),
    (open, stopping, bound): (bool, bool, Duration),
    destroy: impl Fn() -> Result<Residue, String> + Send + 'static,
    hooks: Option<Hooks>,
) -> Ended {
    let had_hooks = hooks.is_some();
    let all = move || {
        let first = open.then(&destroy);
        let refusals = hooks.map(Hooks::stop).unwrap_or_default();
        let residue = match first {
            Some(r) if !had_hooks => r,
            _ => destroy(),
        };
        (Some(residue), refusals)
    };
    if !stopping {
        return all();
    }
    let say: Say = Arc::new(|l| eprintln!("{l}"));
    let overdue = |l: String| match crate::cancel::by_signal() {
        true => crate::cancel::end_now(&format!("{l}; podrunner ends now, and a helper ends the unit")),
        false => eprintln!("{l}; the run goes on, and the unit goes in the background"),
    };
    let pace = Pace { tick: TICK, bound: Some(bound), overdue: &overdue };
    wait_for(job, unit, pace, &say, all).unwrap_or((None, Vec::new()))
}

#[cfg(test)]
mod tests;
