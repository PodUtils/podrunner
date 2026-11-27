//! The cancel of a local run at SIGINT, SIGTERM or SIGHUP (T-141, T-273).
//! The handler only sets a flag: the drive loop then cancels the running job (the executor
//! ends the runner, or destroys its unit), prints `cancelled` for each job
//! that did not end, and the run removes its scratch directories. A third
//! signal ends the process at once: it ends the process group of each
//! runner, and starts the helper of [`at_once`], which removes the units
//! and the files of the run when the process is gone (T-636).

use std::ffi::{CString, c_char};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering};

/// Set by the first SIGINT, SIGTERM or SIGHUP.
pub static SIGNALLED: AtomicBool = AtomicBool::new(false);
static COUNT: AtomicU8 = AtomicU8::new(0);

/// The process group of each runner on this host that runs now (0: a free
/// slot): the third signal ends each one, as the stop of a job does.
static GROUPS: [AtomicI32; 16] = [const { AtomicI32::new(0) }; 16];

/// The helper that the third signal starts, made before the signal: the
/// handler can only call functions that are async-signal-safe, so it
/// allocates nothing.
struct Helper {
    program: CString,
    _args: Vec<CString>,
    argv: Vec<*const c_char>,
    started: Vec<u8>,
    failed: Vec<u8>,
}

// SAFETY: the pointers of `argv` point into `_args`, which the helper owns
// and never changes after it is made.
unsafe impl Send for Helper {}
// SAFETY: as above; the handler only reads.
unsafe impl Sync for Helper {}

static HELPER: OnceLock<Helper> = OnceLock::new();

/// The line of a third signal with no helper.
const STAY: &[u8] = b"podrunner: ended at once; the units and the files of the run stay in TMPDIR: \
the next `podrunner run` with the same TMPDIR ends them\n";

/// Set the helper of the third signal (T-636): `program` with `args` runs
/// in a new session when the third signal ends this process. `started` is
/// the line of a helper that started, `failed` the line of one that did
/// not start. Set once; a later call changes nothing. An argument with a
/// NUL byte sets no helper.
pub fn at_once(program: &str, args: &[String], started: &str, failed: &str) {
    let Ok(program) = CString::new(program) else { return };
    let Ok(args) = args.iter().map(|a| CString::new(a.as_str())).collect::<Result<Vec<_>, _>>() else { return };
    let mut argv: Vec<*const c_char> = args.iter().map(|a| a.as_ptr()).collect();
    argv.push(std::ptr::null());
    let line = |s: &str| format!("{}\n", s.trim_end()).into_bytes();
    let _ = HELPER.set(Helper { program, _args: args, argv, started: line(started), failed: line(failed) });
}

/// The hold of the process group of a runner ([`hold_group`]); the drop
/// frees it.
pub struct HeldGroup(i32);

impl Drop for HeldGroup {
    fn drop(&mut self) {
        let g = self.0;
        if g > 0 {
            let _ = GROUPS.iter().find(|s| s.compare_exchange(g, 0, Ordering::SeqCst, Ordering::SeqCst).is_ok());
        }
    }
}

/// Hold the process group `pgid` of a runner until the drop of the hold: a
/// third signal ends it. With each slot in use, the group is not held.
pub fn hold_group(pgid: u32) -> HeldGroup {
    let g = i32::try_from(pgid).unwrap_or(0);
    let held = g > 0 && GROUPS.iter().any(|s| s.compare_exchange(0, g, Ordering::SeqCst, Ordering::SeqCst).is_ok());
    HeldGroup(if held { g } else { 0 })
}

fn say(m: &[u8]) {
    // SAFETY: write is async-signal-safe.
    unsafe { libc::write(2, m.as_ptr().cast(), m.len()) };
}

/// The bound of the wait for the end of one unit at a cancel by a signal
/// (T-716, a call in `docs/decisions.md`): the stop time of a cancelled
/// job of GitHub with a margin; the end of a podvm unit takes 15 s or more.
/// A unit that does not go in this time does not go by itself.
pub const UNIT_BOUND: std::time::Duration = std::time::Duration::from_secs(120);

/// A shorter bound in milliseconds, 0 for none: only the private first
/// argument [`BOUND_FLAG`] sets it.
static BOUND_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The private first argument of the binary that sets a short bound for a
/// test (`--podrunner-unit-bound-ms MS`). No document names it.
pub const BOUND_FLAG: &str = "--podrunner-unit-bound-ms";

/// Set the bound of [`unit_bound`] to `ms` milliseconds, for a test.
pub fn set_unit_bound_ms(ms: u64) {
    BOUND_MS.store(ms, Ordering::SeqCst);
}

/// The bound of the wait for one unit at a cancel: [`UNIT_BOUND`], or the
/// bound of [`set_unit_bound_ms`].
pub fn unit_bound() -> std::time::Duration {
    match BOUND_MS.load(Ordering::SeqCst) {
        0 => UNIT_BOUND,
        ms => std::time::Duration::from_millis(ms),
    }
}

/// End the run now, as a third signal does, after the line `why` (T-716):
/// a unit did not go in its bound. The helper ends the units when the
/// process is gone. The exit code is 130, the code of a cancel.
pub fn end_now(why: &str) -> ! {
    say(format!("{}\n", why.trim_end()).as_bytes());
    end_at_once()
}

/// A signal stopped the run: not a change of the tree, not `fail-fast`.
pub fn by_signal() -> bool {
    COUNT.load(Ordering::SeqCst) > 0
}

/// The third signal: end each runner group, start the helper, and end.
/// Each call is async-signal-safe.
fn end_at_once() -> ! {
    for s in &GROUPS {
        let g = s.load(Ordering::SeqCst);
        if g > 0 {
            // SAFETY: kill is async-signal-safe; the group is one that the
            // run made.
            unsafe { libc::kill(-g, libc::SIGKILL) };
        }
    }
    match HELPER.get() {
        None => say(STAY),
        Some(h) => {
            // A raw clone, not fork(3): fork takes the locks of the libc,
            // which the interrupted thread can hold. The child calls only
            // setsid and execve.
            // SAFETY: clone with SIGCHLD and no stack is fork(2); the child
            // runs only async-signal-safe calls, then execve or _exit.
            let pid = unsafe { libc::syscall(libc::SYS_clone, libc::SIGCHLD as libc::c_long, 0, 0, 0, 0) };
            if pid == 0 {
                let env: [*const c_char; 1] = [std::ptr::null()];
                // SAFETY: setsid, execve and _exit are async-signal-safe;
                // `argv` ends with a null pointer.
                unsafe {
                    // A new session: a signal of the terminal does not reach it.
                    libc::setsid();
                    libc::execve(h.program.as_ptr(), h.argv.as_ptr(), env.as_ptr());
                    libc::_exit(127);
                }
            }
            say(if pid > 0 { &h.started } else { &h.failed });
        }
    }
    // SAFETY: _exit is async-signal-safe.
    unsafe { libc::_exit(130) }
}

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
    // The same lines for each signal: after SIGHUP a file or a pipe keeps
    // them, and a terminal that is gone gets them in /dev/null (T-636).
    if n == 0 {
        // The end of a unit can take a while: say what happens (T-312).
        say(b"podrunner: stopping: the running job ends and its unit goes (a third signal ends podrunner at once)\n");
    }
    if n == 1 {
        // A second signal is not lost: say that the stop goes on (T-639).
        say(b"podrunner: stopping: already stopping; a third signal ends podrunner at once\n");
    }
    if n >= 2 {
        end_at_once();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn held(g: i32) -> usize {
        GROUPS.iter().filter(|s| s.load(Ordering::SeqCst) == g).count()
    }

    /// T-636: a hold puts the group in a slot, and its drop frees the
    /// slot; a pid of 0 or past `i32` is not held. Made-up pids above the
    /// limit of the kernel.
    #[test]
    fn a_held_group_is_freed_at_the_drop() {
        let (a, b) = (hold_group(4_194_401), hold_group(4_194_402));
        assert_eq!((held(4_194_401), held(4_194_402)), (1, 1));
        drop(a);
        assert_eq!((held(4_194_401), held(4_194_402)), (0, 1));
        drop(b);
        assert_eq!(held(4_194_402), 0);
        assert_eq!((hold_group(0).0, hold_group(u32::MAX).0), (0, 0));
    }
}
