//! The text of a child process that a signal ended (T-144, T-632). A
//! status with no exit code is a signal: "exit -1" names no cause.

use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;

/// `ended by NAME (N)` when a signal ended `status`, else None. SIGXFSZ
/// adds its cause and its fix: a small limit of file size ends the runner
/// and a podvm load.
pub fn ended_by(status: &ExitStatus) -> Option<String> {
    let s = status.signal()?;
    Some(match (s, name(s)) {
        (libc::SIGXFSZ, _) => {
            "ended by SIGXFSZ: a limit of file size (ulimit -f) is too small; fix: ulimit -f unlimited".into()
        }
        (s, Some(n)) => format!("ended by {n} ({s})"),
        (s, None) => format!("ended by signal {s}"),
    })
}

fn name(s: i32) -> Option<&'static str> {
    Some(match s {
        libc::SIGHUP => "SIGHUP",
        libc::SIGINT => "SIGINT",
        libc::SIGQUIT => "SIGQUIT",
        libc::SIGILL => "SIGILL",
        libc::SIGABRT => "SIGABRT",
        libc::SIGBUS => "SIGBUS",
        libc::SIGFPE => "SIGFPE",
        libc::SIGKILL => "SIGKILL",
        libc::SIGSEGV => "SIGSEGV",
        libc::SIGPIPE => "SIGPIPE",
        libc::SIGALRM => "SIGALRM",
        libc::SIGTERM => "SIGTERM",
        libc::SIGXCPU => "SIGXCPU",
        libc::SIGSYS => "SIGSYS",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    #[test]
    fn a_signal_is_named_and_sigxfsz_has_its_fix() {
        let xfsz = super::ended_by(&ExitStatus::from_raw(libc::SIGXFSZ)).unwrap();
        assert!(xfsz.starts_with("ended by SIGXFSZ") && xfsz.ends_with("fix: ulimit -f unlimited"), "{xfsz}");
        assert_eq!(super::ended_by(&ExitStatus::from_raw(libc::SIGKILL)).unwrap(), "ended by SIGKILL (9)");
        assert_eq!(super::ended_by(&ExitStatus::from_raw(40)).unwrap(), "ended by signal 40");
        // An exit is not a signal.
        assert_eq!(super::ended_by(&ExitStatus::from_raw(1 << 8)), None);
    }
}
