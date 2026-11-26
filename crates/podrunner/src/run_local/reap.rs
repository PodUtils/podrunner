//! The helper of a third signal (T-636). A third signal ends `podrunner
//! run` at once: no destroy of a unit and no removal of a directory runs.
//! The handler starts this binary again as a helper in a new session; the
//! helper waits until the run is gone, then ends the units of each dead run
//! and removes its files, as the next run does ([`super::stale`]).

use std::path::Path;
use std::time::{Duration, Instant};

/// The private first argument of the helper.
pub const FLAG: &str = "--podrunner-end-run";

/// How long the helper waits for the end of the run.
const WAIT: Duration = Duration::from_secs(30);

/// Make the helper of the third signal of this process: this binary with
/// [`FLAG`], `TMPDIR` and the pid of the run. The handler passes no
/// environment, so `TMPDIR` is an argument.
pub fn prepare() {
    let tmp = std::env::temp_dir().display().to_string();
    let args = [String::from("podrunner"), FLAG.into(), tmp.clone(), std::process::id().to_string()];
    let started =
        format!("podrunner: ended at once; a helper ends the units of the run and removes its files in {tmp}");
    let failed = format!(
        "podrunner: ended at once; the units and the files of the run stay in {tmp}: \
         the next `podrunner run` with TMPDIR={tmp} ends them"
    );
    podrunner_local::cancel::at_once("/proc/self/exe", &args, &started, &failed);
}

/// The helper: `ARGS` are `TMPDIR PID`. Wait until the process `PID` is
/// gone or a zombie, at most [`WAIT`], then end each dead run in `TMPDIR`.
/// Exit 0, or 64 for other arguments.
pub fn helper(args: &[String]) -> u8 {
    let [tmp, pid] = args else { return 64 };
    let Ok(pid) = pid.parse::<u32>() else { return 64 };
    let t = Instant::now();
    let gone = || podrunner_unit::procs::stat(pid).is_none_or(|s| matches!(s.state, 'Z' | 'X'));
    while !gone() {
        if t.elapsed() > WAIT {
            eprintln!("podrunner: the run {pid} did not end in {} s; the next run ends its units", WAIT.as_secs());
            return 0;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // SAFETY: getuid has no failure.
    let uid = unsafe { libc::getuid() };
    super::stale::end_dead_runs_in(Path::new(tmp), uid, super::stale::here());
    0
}
