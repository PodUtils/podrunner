//! The processes of a unit on the host. Each process of a unit carries the
//! tag of the unit in its environment (`PODRUNNER_UNIT_TAG`); a scan of
//! `/proc` finds them, also a `setsid` grandchild whose parent is gone.

use std::fs;
use std::process::ExitStatus;
use std::time::{Duration, Instant};

use podrunner_unit::Exit;

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
        .filter(|pid| podrunner_unit::conformance::alive(*pid))
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

/// A tag that no other unit of this host has: the process id, the time
/// and the unit id.
pub fn new_tag(unit: &str) -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{}-{nanos}-{unit}", std::process::id())
}
