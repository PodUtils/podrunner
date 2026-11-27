//! The key of a run process in the names of its files in `TMPDIR`. Two
//! containers that share `TMPDIR` can each have a run as pid 1 (T-607), and
//! two hosts can share it (T-615): a pid alone is not unique. [`here`] gives
//! the pid namespace of this process as a key ([`ns_key`]) and its pid; the
//! directories of a run carry both. The files of each unit carry [`key`], a
//! short hash of the two (T-715): one run never makes or removes a file of
//! another run, and the cleanup of a dead run finds its files by that key.

use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

/// The socket directory of a unit: `PREFIX KEY-UNIT`, and its logs at
/// `PREFIX KEY-UNIT.logs`.
pub const UNIT_DIR: &str = "podrunner-local-unit-";
/// The socket of the broker of a unit: `PREFIX KEY-N.sock`.
pub const BROKER: &str = "podrunner-local-broker-";
/// The work directory of a unit with `--export`: `PREFIX KEY-UNIT`.
pub const EXPORT: &str = "podrunner-run-export-";
/// The length of each [`key`]: the length of the largest pid of Linux
/// (4194304), so a name is not longer than a name with a pid.
pub const KEY_LEN: usize = 7;

/// A process as `/proc` sees it: the key of its pid namespace ([`ns_key`])
/// and its pid in that namespace.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Here {
    pub ns: u64,
    pub pid: u32,
}

/// This process, or `None` when `/proc` is not the one of its pid
/// namespace (no `/proc`, or a mount of another namespace): a pid of
/// `/proc` then names another process, so no run counts as dead. Also
/// `None` with no machine id and no boot id ([`ns_key`]): then a run of
/// another host is not known.
pub fn here() -> Option<Here> {
    let pid = std::process::id();
    let seen: u32 = std::fs::read_link("/proc/self").ok()?.to_str()?.parse().ok()?;
    let inode = std::fs::metadata("/proc/self/ns/pid").ok()?.ino();
    let machine = std::fs::read_to_string("/etc/machine-id").ok();
    let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok();
    let ns = ns_key(inode, machine.as_deref(), boot.as_deref())?;
    (seen == pid).then_some(Here { ns, pid })
}

/// The key of a pid namespace: FNV-1a of 64 bits of the machine id and
/// the inode, a fixed key for each pair. An inode is unique on one kernel
/// only: two hosts that share one `TMPDIR` (NFS, a shared home) can give
/// one inode and one pid, and their machine ids differ (T-615). The
/// machine id stays over a reboot, so a run of an earlier boot of this
/// machine is found dead; a pid that a new process took only keeps it.
/// With no machine id (no file, or not 32 hex digits), the boot id; with
/// neither, `None`. Two hosts with one copied machine id are one host.
pub fn ns_key(inode: u64, machine: Option<&str>, boot: Option<&str>) -> Option<u64> {
    let machine = machine.map(str::trim).filter(|m| m.len() == 32 && m.bytes().all(|b| b.is_ascii_hexdigit()));
    let (tag, id) = match (machine, boot.map(str::trim).filter(|b| !b.is_empty())) {
        (Some(m), _) => (b'm', m),
        (None, Some(b)) => (b'b', b),
        (None, None) => return None,
    };
    Some(fnv([tag].into_iter().chain(id.bytes()).chain([0]).chain(inode.to_le_bytes())))
}

fn fnv(bytes: impl Iterator<Item = u8>) -> u64 {
    bytes.fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3))
}

/// The digits of a key: 32 of them, 5 bits each. `x` is not one of them.
const DIGITS: &[u8; 32] = b"0123456789abcdefghijklmnopqrstuv";

/// The key of the process `here`: [`KEY_LEN`] digits of a hash of its pid
/// namespace and its pid (35 bits), so the pid 1 of two containers gets two
/// keys. With no `here`, `x` and digits of the time: unique, and no
/// cleanup finds it, as a run name of no namespace.
pub fn key_of(here: Option<Here>) -> String {
    let (first, mut v) = match here {
        Some(h) => (None, fnv(h.ns.to_le_bytes().into_iter().chain(h.pid.to_le_bytes()))),
        None => {
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            (Some('x'), fnv(t.as_nanos().to_le_bytes().into_iter().chain(std::process::id().to_le_bytes())))
        }
    };
    let mut s: String = first.into_iter().collect();
    while s.len() < KEY_LEN {
        s.push(char::from(DIGITS[(v & 31) as usize]));
        v >>= 5;
    }
    s
}

/// The key of this process ([`key_of`] of [`here`]), made once.
pub fn key() -> &'static str {
    static KEY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    KEY.get_or_init(|| key_of(here()))
}

/// `TMPDIR/PREFIX KEY-REST`: a file of a unit of this process.
pub fn in_tmp(prefix: &str, rest: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}{}-{rest}", key()))
}

/// The socket of the egress module of a unit behind its front, in the
/// socket directory of the unit.
pub const EGRESS_INNER: &str = "egress-inner.sock";

/// The sockets that the unit `n` of this process binds in `TMPDIR`: a
/// unit behind a front (`fronted`, a guest or the vnet) has the socket of
/// its broker and the front; a unit with an egress module (`egress`) has
/// its socket. A driver, podnet, the vnet shim or a guest reaches each one
/// by its path, so each must fit `sun_path`. A long `TMPDIR` passes that
/// limit: the error names the path and the fix (T-738).
pub fn sockets_fit(n: u64, fronted: bool, egress: bool) -> Result<(), String> {
    use podrunner_unit::socket::{TMPDIR_FIX, fits};
    let dir = in_tmp(UNIT_DIR, &format!("u-{n}"));
    let sockets = [
        (fronted, in_tmp(BROKER, &format!("{n}.sock"))),
        (fronted || egress, dir.join(podrunner_unit::EGRESS_SOCKET)),
        (fronted && egress, dir.join(EGRESS_INNER)),
    ];
    sockets.iter().filter(|(on, _)| *on).try_for_each(|(_, p)| fits(p, TMPDIR_FIX))
}

/// The key in a name of a file of a unit (each prefix above), or `None`.
pub fn key_in(name: &str) -> Option<&str> {
    let rest = [UNIT_DIR, BROKER, EXPORT].iter().find_map(|p| name.strip_prefix(p))?;
    let (key, _) = rest.split_once('-')?;
    (key.len() == KEY_LEN).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-715: pid 1 of two pid namespaces gets two keys, so two runs of
    /// one pid that share `TMPDIR` make two names; one process keeps one
    /// key, and its names give the key back.
    #[test]
    fn two_runs_of_one_pid_get_two_keys() {
        let (a, b) = (Here { ns: 4026531836, pid: 1 }, Here { ns: 4026532266, pid: 1 });
        assert_ne!(key_of(Some(a)), key_of(Some(b)));
        assert_ne!(key_of(Some(a)), key_of(Some(Here { ns: a.ns, pid: 2 })));
        assert_eq!(key_of(Some(a)), key_of(Some(a)));
        assert_eq!(key(), key());
        let none = key_of(None);
        assert!(none.starts_with('x') && none.len() == KEY_LEN, "{none}");
        for h in [a, b, Here { ns: 0, pid: 0 }, Here { ns: u64::MAX, pid: u32::MAX }] {
            let k = key_of(Some(h));
            assert!(k.len() == KEY_LEN && !k.contains(['x', '-']), "{k}");
        }
        let unit = in_tmp(UNIT_DIR, "u-1");
        assert_eq!(unit.file_name().unwrap().to_str().unwrap(), format!("podrunner-local-unit-{}-u-1", key()));
        for p in [unit, in_tmp(BROKER, "3.sock"), in_tmp(EXPORT, "u-12")] {
            assert_eq!(key_in(p.file_name().unwrap().to_str().unwrap()), Some(key()), "{}", p.display());
        }
        for name in ["podrunner-local-unit-1-u-1", "podrunner-local-unit-", "podrunner-run-units-1-2-0", "x"] {
            assert_eq!(key_in(name), None, "{name}");
        }
    }

    /// T-715: a key is not longer than the largest pid, so the egress
    /// socket of a unit fits `sun_path` (107 bytes) with each `TMPDIR` that
    /// fitted with a pid: at that longest `TMPDIR` it fits, one byte more
    /// does not.
    #[test]
    fn a_socket_path_at_the_longest_tmpdir_fits() {
        const SUN_PATH_MAX: usize = 107;
        let old = format!("/{UNIT_DIR}4194304-u-999/{}", podrunner_unit::EGRESS_SOCKET);
        let longest = SUN_PATH_MAX - old.len();
        for k in [key_of(None), key_of(Some(Here { ns: 1, pid: 4194304 }))] {
            for (tmp, fits) in [(longest, true), (longest + 1, false)] {
                let tmp = format!("/{}", "t".repeat(tmp - 1));
                let egress = format!("{tmp}/{UNIT_DIR}{k}-u-999/{}", podrunner_unit::EGRESS_SOCKET);
                let broker = format!("{tmp}/{BROKER}{k}-999.sock");
                assert_eq!(egress.len() <= SUN_PATH_MAX, fits, "{egress}");
                assert!(broker.len() < egress.len(), "{broker}");
            }
        }
    }
}
