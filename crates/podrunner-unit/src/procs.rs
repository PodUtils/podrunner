//! The processes of a unit on the host, for the drivers whose units are
//! processes of this host (`fake`, `podvm`). Each process of a unit carries
//! the tag of the unit in its environment (`PODRUNNER_UNIT_TAG`); a scan of
//! `/proc` finds them, also a `setsid` grandchild whose parent is gone.

use std::fs;
use std::process::ExitStatus;
use std::time::{Duration, Instant};

use crate::Exit;

/// The variable that holds the tag of a unit.
pub const TAG_VAR: &str = "PODRUNNER_UNIT_TAG";

/// The live processes (not zombies) whose environment holds `TAG_VAR=tag`.
pub fn tagged(tag: &str) -> Vec<u32> {
    let needle = format!("{TAG_VAR}={tag}");
    let Ok(dir) = fs::read_dir("/proc") else { return Vec::new() };
    let mut pids: Vec<u32> = dir
        .filter_map(|e| e.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            fs::read(format!("/proc/{pid}/environ"))
                .is_ok_and(|env| env.split(|b| *b == 0).any(|entry| entry == needle.as_bytes()))
        })
        .filter(|pid| crate::conformance::alive(*pid))
        .collect();
    pids.sort_unstable();
    pids
}

pub fn kill(pid: u32, signal: i32) {
    if let Ok(pid) = libc::pid_t::try_from(pid) {
        // SAFETY: kill(2) takes plain integers; a pid that is gone gives ESRCH.
        unsafe { libc::kill(pid, signal) };
    }
}

/// Send SIGKILL to each tagged process until none is left or the limit
/// ends; a process can fork while the kill runs. Give the count left.
pub fn kill_tagged(tag: &str, limit: Duration) -> u64 {
    let end = Instant::now() + limit;
    loop {
        let pids = tagged(tag);
        if pids.is_empty() || Instant::now() >= end {
            return pids.len() as u64;
        }
        for p in pids {
            kill(p, libc::SIGKILL);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub fn exit_of(status: ExitStatus) -> Exit {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(c), _) => Exit::Code(c),
        (None, Some(s)) => Exit::Signal(s),
        (None, None) => Exit::Code(-1),
    }
}

/// The tag of a unit: a hash of the state directory of the driver and the
/// unit id. A driver that starts again on the same directory finds the
/// processes of a unit that an earlier process started (the restart of
/// podrunner).
pub fn tag_of(state_dir: &std::path::Path, unit: &str) -> String {
    // FNV-1a: stable across processes and versions.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in state_dir.as_os_str().as_encoded_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}-{unit}")
}
