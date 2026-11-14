//! The cancel of a local run at SIGINT or SIGTERM (T-141). The handler only
//! sets a flag: the drive loop then cancels the running job (the executor
//! ends the runner, or destroys its unit), prints `cancelled` for each job
//! that did not end, and the run removes its scratch directories. A third
//! signal ends the process at once.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Set by the first SIGINT or SIGTERM.
pub static SIGNALLED: AtomicBool = AtomicBool::new(false);
static COUNT: AtomicU8 = AtomicU8::new(0);

extern "C" fn on_signal(_: libc::c_int) {
    SIGNALLED.store(true, Ordering::SeqCst);
    if COUNT.fetch_add(1, Ordering::SeqCst) >= 2 {
        // SAFETY: _exit is async-signal-safe.
        unsafe { libc::_exit(130) };
    }
}

/// Install the handler of SIGINT and SIGTERM.
pub fn install() {
    for sig in [libc::SIGINT, libc::SIGTERM] {
        // SAFETY: the handler only touches atomics and calls _exit.
        unsafe {
            let mut act: libc::sigaction = std::mem::zeroed();
            act.sa_sigaction = on_signal as *const () as usize;
            libc::sigemptyset(&mut act.sa_mask);
            libc::sigaction(sig, &act, std::ptr::null_mut());
        }
    }
}

/// A signal asked the run to stop.
pub fn requested() -> bool {
    SIGNALLED.load(Ordering::SeqCst)
}
