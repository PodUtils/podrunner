//! The cancel of a local run at SIGINT, SIGTERM or SIGHUP (T-141, T-273).
//! The handler only
//! sets a flag: the drive loop then cancels the running job (the executor
//! ends the runner, or destroys its unit), prints `cancelled` for each job
//! that did not end, and the run removes its scratch directories. A third
//! signal ends the process at once.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Set by the first SIGINT or SIGTERM.
pub static SIGNALLED: AtomicBool = AtomicBool::new(false);
static COUNT: AtomicU8 = AtomicU8::new(0);

extern "C" fn on_signal(sig: libc::c_int) {
    if sig == libc::SIGHUP {
        // The terminal is gone: a write to it would fail and stop the
        // cleanup, so the output goes to /dev/null.
        // SAFETY: open and dup2 are async-signal-safe.
        unsafe {
            // Only a terminal is gone: a file or a pipe keeps the lines.
            let null = libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY);
            if null >= 0 {
                for fd in [1, 2] {
                    if libc::isatty(fd) == 1 {
                        libc::dup2(null, fd);
                    }
                }
            }
        }
    }
    SIGNALLED.store(true, Ordering::SeqCst);
    let n = COUNT.fetch_add(1, Ordering::SeqCst);
    if n == 0 && sig != libc::SIGHUP {
        // The end of a unit can take a while: say what happens (T-312).
        let m =
            b"podrunner: stopping: the running job ends and its unit goes (a third signal ends podrunner at once)\n";
        // SAFETY: write is async-signal-safe.
        unsafe { libc::write(2, m.as_ptr().cast(), m.len()) };
    }
    if n == 1 && sig != libc::SIGHUP {
        // A second signal is not lost: say that the stop goes on (T-639).
        let m = b"podrunner: stopping: already stopping; a third signal ends podrunner at once\n";
        // SAFETY: write is async-signal-safe.
        unsafe { libc::write(2, m.as_ptr().cast(), m.len()) };
    }
    if n >= 2 {
        // SAFETY: _exit is async-signal-safe.
        unsafe { libc::_exit(130) };
    }
}

/// Install the handler of SIGINT, SIGTERM and SIGHUP.
pub fn install() {
    for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        // SAFETY: the handler only touches atomics and calls _exit.
        unsafe {
            let mut act: libc::sigaction = std::mem::zeroed();
            act.sa_sigaction = on_signal as *const () as usize;
            libc::sigemptyset(&mut act.sa_mask);
            libc::sigaction(sig, &act, std::ptr::null_mut());
        }
    }
}

/// Ask the run to stop as a signal does: a change of the tree under
/// `--watch` (T-142).
pub fn request() {
    BY_CHANGE.store(true, Ordering::SeqCst);
    SIGNALLED.store(true, Ordering::SeqCst);
}

/// Set by [`request`]: a change of the tree, not a signal, stopped the run.
static BY_CHANGE: AtomicBool = AtomicBool::new(false);

/// A change of the tree stopped the run, not a signal (T-278).
pub fn by_change() -> bool {
    BY_CHANGE.load(Ordering::SeqCst) && COUNT.load(Ordering::SeqCst) == 0
}

/// Clear a request, for the next run of `--watch` or `--schedule`.
pub fn reset() {
    SIGNALLED.store(false, Ordering::SeqCst);
    BY_CHANGE.store(false, Ordering::SeqCst);
    COUNT.store(0, Ordering::SeqCst);
}

/// The stop flag of one job (T-132): `fail-fast` cancels a running cell.
pub type Stop = std::sync::Arc<AtomicBool>;

/// A signal asked the run to stop, or `stop` asked the job to stop.
pub fn stopped(stop: &Option<Stop>) -> bool {
    requested() || stop.as_ref().is_some_and(|s| s.load(Ordering::SeqCst))
}

/// A signal asked the run to stop.
pub fn requested() -> bool {
    SIGNALLED.load(Ordering::SeqCst)
}
