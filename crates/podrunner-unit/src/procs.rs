//! The processes of a unit on the host, for the drivers whose units are
//! processes of this host (`fake`, `podvm`). Each process of a unit carries
//! the tag of the unit in its environment (`PODRUNNER_UNIT_TAG`); a scan of
//! `/proc` finds them, also a `setsid` grandchild whose parent is gone.

use std::collections::{BTreeMap, BTreeSet};
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
    kill_until_gone(|| tagged(tag), limit)
}

/// The state letter and the start time (clock ticks after boot) of a
/// process, from `/proc/PID/stat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    pub state: char,
    pub start: u64,
}

/// The [`Stat`] of `pid`; `None` when it is gone.
pub fn stat(pid: u32) -> Option<Stat> {
    parse_stat(&fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

fn parse_stat(stat: &str) -> Option<Stat> {
    // The name in parentheses can hold ')' and spaces.
    let mut rest = stat.rsplit_once(')')?.1.split_whitespace();
    let state = rest.next()?.chars().next()?;
    // Field 22 of stat; the state is field 3.
    let start = rest.nth(18)?.parse().ok()?;
    Some(Stat { state, start })
}

/// SIGKILL to each process that `list` gives until `list` is empty and
/// each signalled process is gone, a zombie or dead, or the limit ends. Give the
/// count left.
pub fn kill_until_gone(list: impl FnMut() -> Vec<u32>, limit: Duration) -> u64 {
    let end = Instant::now() + limit;
    let probe = Probe {
        list,
        stat,
        kill: |p| kill(p, libc::SIGKILL),
        ended: || Instant::now() >= end,
        pause: || std::thread::sleep(Duration::from_millis(10)),
    };
    kill_wait(probe)
}

/// The view of the host that [`kill_wait`] takes; a test gives its own.
pub(crate) struct Probe<L, S, K, E, P> {
    pub list: L,
    pub stat: S,
    pub kill: K,
    pub ended: E,
    pub pause: P,
}

pub(crate) fn kill_wait<L, S, K, E, P>(mut p: Probe<L, S, K, E, P>) -> u64
where
    L: FnMut() -> Vec<u32>,
    S: Fn(u32) -> Option<Stat>,
    K: FnMut(u32),
    E: FnMut() -> bool,
    P: FnMut(),
{
    // A killed process frees its memory map first, and its environment
    // then reads empty: `list` drops it while it still runs. So each
    // signalled pid counts until it is gone or a zombie; its start time
    // tells a reused pid.
    let mut signalled: BTreeMap<u32, u64> = BTreeMap::new();
    loop {
        let pids = (p.list)();
        signalled
            .retain(|pid, start| (p.stat)(*pid).is_some_and(|s| s.start == *start && !matches!(s.state, 'Z' | 'X')));
        let mut left: BTreeSet<u32> = pids.iter().copied().collect();
        left.extend(signalled.keys());
        if left.is_empty() || (p.ended)() {
            return left.len() as u64;
        }
        for pid in pids {
            if let Some(s) = (p.stat)(pid) {
                signalled.entry(pid).or_insert(s.start);
            }
            (p.kill)(pid);
        }
        (p.pause)();
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

#[cfg(test)]
#[path = "procs_tests.rs"]
mod tests;
