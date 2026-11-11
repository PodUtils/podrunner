//! The service manager contract of `serve` (GitHub #60): SIGTERM and SIGINT
//! start the drain of `podrunner stop`, and `sd_notify` datagrams say
//! READY, STOPPING and WATCHDOG. With no `NOTIFY_SOCKET` nothing is sent.

use std::os::unix::net::UnixDatagram;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static SIGNALLED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    // Only an atomic store: the one thing a handler can do safely.
    SIGNALLED.store(true, Ordering::SeqCst);
}

/// Catch SIGTERM and SIGINT: each sets a flag that `take_signal` reads.
pub fn install() {
    let handler = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
    for sig in [libc::SIGTERM, libc::SIGINT] {
        // SAFETY: the handler only stores to an atomic.
        unsafe { libc::signal(sig, handler) };
    }
}

/// Whether a stop signal came since the last call.
pub fn take_signal() -> bool {
    SIGNALLED.swap(false, Ordering::SeqCst)
}

/// Send `state` to the socket of `NOTIFY_SOCKET`, a path or `@NAME` (an
/// abstract name). A missing variable sends nothing.
pub fn send(state: &str) {
    let Some(addr) = std::env::var_os("NOTIFY_SOCKET") else { return };
    let addr = addr.to_string_lossy().into_owned();
    let _ = send_to(&addr, state);
}

fn send_to(addr: &str, state: &str) -> std::io::Result<()> {
    let sock = UnixDatagram::unbound()?;
    match addr.strip_prefix('@') {
        Some(name) => {
            use std::os::linux::net::SocketAddrExt;
            let a = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes())?;
            sock.send_to_addr(state.as_bytes(), &a)?;
        }
        None => {
            sock.send_to(state.as_bytes(), addr)?;
        }
    }
    Ok(())
}

/// The watchdog interval: half of `WATCHDOG_USEC`, when `WATCHDOG_PID` is
/// this process or unset.
pub fn watchdog() -> Option<Duration> {
    let usec: u64 = std::env::var("WATCHDOG_USEC").ok()?.parse().ok()?;
    if let Ok(pid) = std::env::var("WATCHDOG_PID")
        && pid.parse::<u32>().ok() != Some(std::process::id())
    {
        return None;
    }
    (usec > 0).then(|| Duration::from_micros(usec / 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_datagram_reaches_a_path_and_an_abstract_name() {
        let dir = std::env::temp_dir().join(format!("podrunner-notify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notify.sock");
        let rx = UnixDatagram::bind(&path).unwrap();
        send_to(path.to_str().unwrap(), "READY=1").unwrap();
        let mut buf = [0u8; 64];
        let n = rx.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"READY=1");

        use std::os::linux::net::SocketAddrExt;
        let name = format!("podrunner-notify-test-{}", std::process::id());
        let a = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes()).unwrap();
        let rx = UnixDatagram::bind_addr(&a).unwrap();
        send_to(&format!("@{name}"), "STOPPING=1").unwrap();
        let n = rx.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"STOPPING=1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_signal_sets_the_flag_once() {
        install();
        assert!(!take_signal());
        // SAFETY: raise(3) sends SIGINT to this process; the handler stores.
        unsafe { libc::raise(libc::SIGINT) };
        assert!(take_signal());
        assert!(!take_signal());
    }
}
