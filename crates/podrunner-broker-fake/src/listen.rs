//! The listener of a broker: a free port of the loopback (T-228), and the
//! end of its loop when the broker goes (#221).

use std::net::TcpListener;

use crate::Broker;

/// The address that wakes the loop of a listener.
pub(crate) enum Wake {
    Tcp(u16),
    Unix(std::path::PathBuf),
}

impl Drop for Broker {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = match &self.wake {
            Wake::Tcp(port) => std::net::TcpStream::connect(("127.0.0.1", *port)).map(drop),
            Wake::Unix(path) => std::os::unix::net::UnixStream::connect(path).map(drop),
        };
    }
}

/// A free port of the loopback and its listener. Under a supervisor that
/// refuses `getsockname` (`podnet pod`), the port of `127.0.0.1:0` cannot
/// be read: then try ports of the dynamic range, from a start that each
/// process draws (T-228).
pub fn bind_loopback() -> std::io::Result<(TcpListener, u16)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    if let Ok(a) = listener.local_addr() {
        return Ok((listener, a.port()));
    }
    drop(listener);
    let mut seed = [0u8; 2];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut seed));
    let start = u16::from_le_bytes(seed) % 16_000;
    let mut last = std::io::Error::from(std::io::ErrorKind::AddrInUse);
    for i in 0..400u16 {
        let port = 49_152 + (start + i * 37) % 16_000;
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(l) => return Ok((l, port)),
            Err(e) => last = e,
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    /// #221: a broker that goes closes its listener: a connect is refused.
    #[test]
    fn a_broker_that_goes_closes_its_listener() {
        // Not in TMPDIR: `sun_path` holds 107 bytes, and a long TMPDIR
        // passes that limit (T-735).
        let dir = std::path::Path::new("/tmp").join(format!("pb-broker-drop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("b.sock");
        let b = crate::Broker::start_unix(&sock, "broker.test").unwrap();
        assert!(std::os::unix::net::UnixStream::connect(&sock).is_ok(), "the broker listens");
        drop(b);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::os::unix::net::UnixStream::connect(&sock).is_ok() {
            assert!(std::time::Instant::now() < deadline, "the listener stays after the broker went");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
