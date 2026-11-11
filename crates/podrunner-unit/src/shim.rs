//! The shim of a unit (T-063): the tree kill with no root and no cgroup.
//! A process can drop the tag of its unit (`env -i`) and leave its
//! session (`setsid`), but it cannot leave the tree of a subreaper: each
//! orphan of the tree comes back to the shim. `destroy` then walks the
//! parent links of `/proc` from the shim.
//!
//! The shim is the child that `Command::spawn` makes: before the exec, it
//! sets `PR_SET_CHILD_SUBREAPER` and forks; the new child execs the
//! payload, and the shim stays and reaps. It never execs, so its argv and
//! environment are those of podrunner.

use std::collections::HashMap;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::procs::{kill, tagged};

/// What the shim does when its payload ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Kill what is left of the tree, then end with the code of the
    /// payload: the unit ends with its top process.
    EndWithPayload,
    /// Reap the orphans until none is left: the payload starts a detached
    /// tree (`podvm run -d`) and ends at once.
    Hold,
}

/// Start `cmd` under a shim of mode `mode`. The pid of the spawned child
/// is the pid of the shim; its exit code is the code of the payload
/// (128 + the signal when a signal ended it).
pub fn under_shim(cmd: &mut Command, mode: Mode) {
    use std::os::unix::process::CommandExt;
    // SAFETY: the closure runs in the child between fork and exec. It
    // calls only functions that are safe there: prctl, fork, close_range,
    // close, waitpid, open, read, kill and _exit. It allocates nothing.
    unsafe {
        cmd.pre_exec(move || shim(mode));
    }
}

/// The body of the shim. It returns only in the payload, which then execs.
unsafe fn shim(mode: Mode) -> std::io::Result<()> {
    // SAFETY: plain system calls on integers and on buffers of this frame.
    unsafe {
        if libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let payload = libc::fork();
        if payload < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if payload == 0 {
            return Ok(());
        }
        // The shim never execs: it closes each descriptor above 2, so that
        // the parent sees the exec of the payload on its pipe.
        if libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 0u32) != 0 {
            for fd in 3..4096 {
                libc::close(fd);
            }
        }
        let mut code = 1;
        let mut ended = false;
        loop {
            if ended && mode == Mode::EndWithPayload {
                kill_children();
            }
            let mut status = 0;
            let w = libc::waitpid(-1, &mut status, 0);
            if w < 0 {
                if *libc::__errno_location() == libc::EINTR {
                    continue;
                }
                // ECHILD: no process is left in the tree.
                libc::_exit(code);
            }
            if w == payload {
                code = if libc::WIFEXITED(status) {
                    libc::WEXITSTATUS(status)
                } else if libc::WIFSIGNALED(status) {
                    128 + libc::WTERMSIG(status)
                } else {
                    1
                };
                ended = true;
            }
        }
    }
}

/// SIGKILL to each child of the shim (`/proc/thread-self/children`).
unsafe fn kill_children() {
    let mut buf = [0u8; 4096];
    // SAFETY: open, read and close on a buffer of this frame.
    unsafe {
        let fd = libc::open(c"/proc/thread-self/children".as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC);
        if fd < 0 {
            return;
        }
        let n = libc::read(fd, buf.as_mut_ptr().cast(), buf.len());
        libc::close(fd);
        let mut pid: libc::pid_t = 0;
        for &b in buf.iter().take(n.max(0) as usize) {
            if b.is_ascii_digit() {
                pid = pid * 10 + libc::pid_t::from(b - b'0');
            } else {
                if pid > 0 {
                    libc::kill(pid, libc::SIGKILL);
                }
                pid = 0;
            }
        }
        if pid > 0 {
            libc::kill(pid, libc::SIGKILL);
        }
    }
}

/// The parent of each live process.
fn parents() -> HashMap<u32, u32> {
    let Ok(dir) = std::fs::read_dir("/proc") else { return HashMap::new() };
    dir.filter_map(|e| e.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            // The state and the parent follow the name in parentheses.
            let mut rest = stat.rsplit_once(')')?.1.split_whitespace();
            let state = rest.next()?;
            let ppid = rest.next()?.parse().ok()?;
            (state != "Z").then_some((pid, ppid))
        })
        .collect()
}

/// The live processes under `root`, at any depth (`root` not included).
pub fn descendants(root: u32) -> Vec<u32> {
    let parents = parents();
    let mut out = Vec::new();
    let mut todo = vec![root];
    while let Some(p) = todo.pop() {
        for (&child, _) in parents.iter().filter(|(_, pp)| **pp == p) {
            out.push(child);
            todo.push(child);
        }
    }
    out.sort_unstable();
    out
}

/// The live children of `pid`.
pub fn children(pid: u32) -> Vec<u32> {
    let mut out: Vec<u32> = parents().into_iter().filter(|(_, pp)| *pp == pid).map(|(p, _)| p).collect();
    out.sort_unstable();
    out
}

/// The processes of a unit: the tree under its shim and each process with
/// its tag. The shim is not one of them.
pub fn unit_pids(shim: u32, tag: Option<&str>) -> Vec<u32> {
    let mut pids = descendants(shim);
    if let Some(t) = tag {
        pids.extend(tagged(t));
    }
    pids.sort_unstable();
    pids.dedup();
    pids.retain(|p| *p != shim);
    pids
}

/// Each process with the tag `tag`, and the tree under each one: a
/// subreaper with the tag (the supervisor of podvm) holds the orphans
/// that dropped the tag.
pub fn tree_of_tagged(tag: &str) -> Vec<u32> {
    let mut pids = tagged(tag);
    for p in pids.clone() {
        pids.extend(descendants(p));
    }
    pids.sort_unstable();
    pids.dedup();
    pids
}

/// SIGKILL to each process of [`tree_of_tagged`] until none is left or the
/// limit ends. Give the count left.
pub fn kill_tree_of_tagged(tag: &str, limit: Duration) -> u64 {
    let end = Instant::now() + limit;
    loop {
        let pids = tree_of_tagged(tag);
        if pids.is_empty() || Instant::now() >= end {
            return pids.len() as u64;
        }
        for p in pids {
            kill(p, libc::SIGKILL);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// SIGKILL to each process of the unit until none is left or the limit
/// ends; the shim lives, so each orphan comes back to it. Give the count
/// left.
pub fn kill_unit(shim: u32, tag: Option<&str>, limit: Duration) -> u64 {
    let end = Instant::now() + limit;
    loop {
        let pids = unit_pids(shim, tag);
        if pids.is_empty() || Instant::now() >= end {
            return pids.len() as u64;
        }
        for p in pids {
            kill(p, libc::SIGKILL);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::alive;

    fn sh(script: &str, mode: Mode) -> std::process::Child {
        let mut c = Command::new("sh");
        c.args(["-c", script]).env("PODRUNNER_UNIT_TAG", "shim-test");
        under_shim(&mut c, mode);
        c.spawn().unwrap()
    }

    #[test]
    fn the_shim_gives_the_code_and_ends_what_is_left() {
        // An orphan with no tag and a session of its own.
        let mut c = sh("(setsid env -i sleep 300 &); exit 7", Mode::EndWithPayload);
        let status = c.wait().unwrap();
        assert_eq!(status.code(), Some(7));
        assert!(descendants(c.id()).is_empty());
    }

    #[test]
    fn an_orphan_with_no_tag_comes_back_to_the_shim_and_is_killed() {
        let mut c = sh("(setsid env -i sleep 300 &); sleep 300", Mode::Hold);
        let end = Instant::now() + Duration::from_secs(10);
        let mut pids = Vec::new();
        while pids.len() < 2 && Instant::now() < end {
            pids = descendants(c.id());
            std::thread::sleep(Duration::from_millis(20));
        }
        // The orphan has no tag and no environment.
        let end = Instant::now() + Duration::from_secs(10);
        while !pids.iter().any(|p| std::fs::read(format!("/proc/{p}/environ")).is_ok_and(|e| e.is_empty())) {
            assert!(Instant::now() < end, "no orphan with an empty environment: {pids:?}");
            std::thread::sleep(Duration::from_millis(20));
            pids = descendants(c.id());
        }
        assert!(pids.iter().any(|p| std::fs::read(format!("/proc/{p}/environ")).is_ok_and(|e| e.is_empty())));
        assert_eq!(kill_unit(c.id(), None, Duration::from_secs(5)), 0);
        assert!(pids.iter().all(|p| !alive(*p)));
        // With no child left, the shim of mode Hold ends.
        assert!(c.wait().unwrap().code().is_some());
    }
}
