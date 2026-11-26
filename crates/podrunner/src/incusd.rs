//! The incusd of the incus kind: its socket, a connection of this user, and
//! the Incus version that limits a container engine in an instance (T-683).

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The time limit of each read and write on the socket of incusd.
const ANSWER: Duration = Duration::from_secs(5);

/// The largest answer of `GET /1.0` that podrunner reads.
const MAX_ANSWER: u64 = 1 << 20;

/// The socket of incusd, as podvm finds it: `PODVM_INCUS_SOCKET`, else
/// `unix.socket` of `INCUS_DIR`, else of `/var/lib/incus`.
pub(crate) fn socket() -> PathBuf {
    if let Some(s) = std::env::var_os("PODVM_INCUS_SOCKET") {
        return s.into();
    }
    let dir = std::env::var_os("INCUS_DIR").map_or_else(|| "/var/lib/incus".into(), PathBuf::from);
    dir.join("unix.socket")
}

/// incusd takes a connection of this user (T-534).
pub(crate) fn connect(socket: &Path) -> Result<(), String> {
    UnixStream::connect(socket).map(drop).map_err(|e| format!("incusd: {}: {}", socket.display(), e.kind()))
}

/// The `server_version` of the incusd at `socket`, from `GET /1.0`.
pub(crate) fn version(socket: &Path) -> Result<String, String> {
    let io = |e: std::io::Error| format!("incusd: {}: {}", socket.display(), e.kind());
    let mut s = UnixStream::connect(socket).map_err(io)?;
    s.set_read_timeout(Some(ANSWER)).map_err(io)?;
    s.set_write_timeout(Some(ANSWER)).map_err(io)?;
    // HTTP/1.0: the answer has no chunks and ends at the close.
    s.write_all(b"GET /1.0 HTTP/1.0\r\nHost: incus\r\nUser-Agent: podrunner\r\n\r\n").map_err(io)?;
    let mut answer = Vec::new();
    s.take(MAX_ANSWER).read_to_end(&mut answer).map_err(io)?;
    server_version(&answer)
}

/// The `server_version` of an answer of `GET /1.0`.
fn server_version(answer: &[u8]) -> Result<String, String> {
    let text = String::from_utf8_lossy(answer);
    let (head, body) = text.split_once("\r\n\r\n").ok_or("incusd: the answer of /1.0 is not HTTP")?;
    let status = head.lines().next().unwrap_or("");
    if status.split_whitespace().nth(1) != Some("200") {
        return Err(format!("incusd: /1.0: {}", crate::text::display(status)));
    }
    let v: serde_json::Value =
        serde_json::from_str(body.trim()).map_err(|e| format!("incusd: the answer of /1.0: {e}"))?;
    v["metadata"]["environment"]["server_version"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "incusd: /1.0 gives no server_version".into())
}

/// The major, minor and patch numbers at the start of a version.
fn numbers(version: &str) -> Option<(u32, u32, u32)> {
    let end = version.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(version.len());
    let mut n = version[..end].split('.').map(|p| p.parse::<u32>().ok());
    let major = n.next()??;
    let minor = n.next().flatten().unwrap_or(0);
    let patch = n.next().flatten().unwrap_or(0);
    Some((major, minor, patch))
}

/// Why a container engine in an instance of this Incus fails, or `None`.
/// runc 1.3.3 and later writes the sysctls of a container through a
/// detached procfs. AppArmor names that path `/sys/...`, and the nesting
/// profile of Incus before 6.0.6 (the LTS line) and 6.19 refuses it
/// (lxc/incus#2623, fixed by lxc/incus#2624). An older runc (an operator
/// engine) is not affected, so the text names the runc. A version that
/// does not parse gives `None`: podrunner does not guess.
pub(crate) fn engine_fault(version: &str) -> Option<String> {
    let v = numbers(version)?;
    let old = v < (6, 0, 6) || (v.0 == 6 && (1..19).contains(&v.1));
    old.then(|| {
        format!(
            "incusd {}: the AppArmor profile of Incus before 6.0.6 and 6.19 refuses the sysctl writes of \
             runc 1.3.3 and later, so an engine with that runc (the engine of runner-full) cannot start a \
             container in an instance (\"open sysctl ... permission denied\", lxc/incus#2623). Upgrade \
             Incus, or run the jobs that use Docker on the microvm kind",
            crate::text::display(version)
        )
    })
}

/// The switch of AppArmor in the kernel. An instance shares the kernel of
/// the host, so with AppArmor off no profile refuses the sysctl writes.
pub(crate) const APPARMOR: &str = "/sys/module/apparmor/parameters/enabled";

/// The check of an engine on the incus kind: `Ok(Some(warning))` for an
/// incusd before the fix, `Ok(None)` when it is new enough or AppArmor is
/// off, and `Err(why)` when incusd gives no version.
pub(crate) fn engine_check(socket: &Path, apparmor: &Path) -> Result<Option<String>, String> {
    let on = std::fs::read_to_string(apparmor).is_ok_and(|s| s.trim_start().starts_with('Y'));
    if !on {
        return Ok(None);
    }
    version(socket).map(|v| engine_fault(&v))
}

/// The warning for an engine on the incus kind with the incusd at `socket`:
/// `None` when incusd is new enough, AppArmor is off or incusd gives no
/// version (`doctor` names that last case).
pub(crate) fn engine_warning(socket: &Path) -> Option<String> {
    engine_check(socket, Path::new(APPARMOR)).ok().flatten()
}

#[cfg(test)]
mod tests;
