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
use std::sync::Arc;
use std::time::Duration;

use crate::procs::{kill_until_gone, tagged};

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
    under_shim_confined(cmd, mode, None);
}

/// [`under_shim`], and the payload applies the rules `confine` (Landlock,
/// seccomp, `no_new_privs`) before its exec (T-042). A refusal of the
/// kernel fails the spawn: no payload runs unconfined.
pub fn under_shim_confined(cmd: &mut Command, mode: Mode, confine: Option<Arc<pod_confine::Compiled>>) {
    use std::os::unix::process::CommandExt;
    // SAFETY: the closure runs in the child between fork and exec. It
    // uses system calls for the fork, cleanup, wait and confinement.
    // It allocates nothing.
    unsafe {
        cmd.pre_exec(move || shim(mode, confine.as_deref()));
    }
}

/// The body of the shim. It returns only in the payload, which then execs.
unsafe fn shim(mode: Mode, confine: Option<&pod_confine::Compiled>) -> std::io::Result<()> {
    // SAFETY: plain system calls on integers and on buffers of this frame.
    unsafe {
        if libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        // The shim owns reaping. Keep exit statuses even when the caller
        // ignored SIGCHLD or installed a handler.
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = libc::SIG_DFL;
        if libc::sigemptyset(&mut action.sa_mask) != 0
            || libc::sigaction(libc::SIGCHLD, &action, std::ptr::null_mut()) != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let payload = libc::fork();
        if payload < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if payload == 0 {
            cloexec_above_2();
            if let Some(c) = confine {
                pod_confine::apply(c).map_err(|r| std::io::Error::from_raw_os_error(r.errno.0))?;
            }
            return Ok(());
        }
        // The shim never execs: it closes each descriptor above 2, so that
        // the parent sees the exec of the payload on its pipe. With no
        // close_range (a kernel before 5.9), each descriptor below the limit
        // of the process (#281): the pipe of the parent can be above 4096.
        if libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 0u32) != 0 {
            for fd in 3..fallback_bound(soft_limit()) {
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

/// The payload keeps no descriptor above 2 past its exec (T-703): a
/// descriptor that the parent got with no `FD_CLOEXEC` (the lock of
/// `flock`) gets the flag. The pipe of `Command` has the flag already, so
/// the parent still sees a failed exec. With no `CLOSE_RANGE_CLOEXEC` (a
/// kernel before 5.11), each descriptor below the limit of the process.
unsafe fn cloexec_above_2() {
    // SAFETY: plain system calls on integers.
    unsafe {
        if libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, libc::CLOSE_RANGE_CLOEXEC) != 0 {
            for fd in 3..fallback_bound(soft_limit()) {
                let flags = libc::fcntl(fd, libc::F_GETFD);
                if flags >= 0 {
                    libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
                }
            }
        }
    }
}

/// The soft limit of open files, or 0 when it is not known.
unsafe fn soft_limit() -> libc::rlim_t {
    let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: getrlimit writes into the struct of this frame.
    match unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) } {
        0 => lim.rlim_cur,
        _ => 0,
    }
}

/// Send SIGKILL to the direct children of the shim.
unsafe fn kill_children() {
    let mut buf = [0u8; 4096];
    // SAFETY: each system call reads or writes this frame only.
    unsafe {
        let fd = libc::open(c"/proc/thread-self/children".as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC);
        if fd >= 0 {
            let got = libc::read(fd, buf.as_mut_ptr().cast(), buf.len());
            libc::close(fd);
            if let Ok(n) = usize::try_from(got)
                && n < buf.len()
            {
                for word in buf[..n].split(u8::is_ascii_whitespace) {
                    if let Some(pid) = pid_of(word) {
                        kill_if_child(pid);
                    }
                }
                return;
            }
        }
        // Some kernels have no children file. Scan proc without allocating.
        let fd = libc::open(c"/proc".as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC);
        if fd < 0 {
            return;
        }
        loop {
            let n = libc::syscall(libc::SYS_getdents64, fd, buf.as_mut_ptr(), buf.len());
            if n < 0 && *libc::__errno_location() == libc::EINTR {
                continue;
            }
            if n <= 0 {
                break;
            }
            let mut entries = &buf[..n as usize];
            while let Some(name) = next_dirent(&mut entries) {
                if let Some(pid) = pid_of(name) {
                    kill_if_child(pid);
                }
            }
        }
        libc::close(fd);
    }
}

/// Read the name and advance past one Linux getdents64 record.
fn next_dirent<'a>(entries: &mut &'a [u8]) -> Option<&'a [u8]> {
    const NAME: usize = 19;
    let len = u16::from_ne_bytes(entries.get(16..18)?.try_into().ok()?) as usize;
    if len <= NAME {
        return None;
    }
    let (entry, rest) = entries.split_at_checked(len)?;
    *entries = rest;
    entry[NAME..].split(|&b| b == 0).next()
}

fn pid_of(word: &[u8]) -> Option<libc::pid_t> {
    if word.is_empty() || !word.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let pid = std::str::from_utf8(word).ok()?.parse().ok()?;
    (pid > 0).then_some(pid)
}

/// Verify the PID with the kernel before signalling it. The single-threaded
/// shim is the only process that can reap its children.
unsafe fn kill_if_child(pid: libc::pid_t) {
    let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::uninit();
    // SAFETY: waitid writes to info. WNOWAIT preserves the child for waitpid.
    unsafe {
        let options = libc::WEXITED | libc::WNOHANG | libc::WNOWAIT;
        loop {
            if libc::waitid(libc::P_PID, pid as libc::id_t, info.as_mut_ptr(), options) == 0 {
                libc::kill(pid, libc::SIGKILL);
                break;
            }
            if *libc::__errno_location() != libc::EINTR {
                break;
            }
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
    kill_until_gone(|| tree_of_tagged(tag), limit)
}

/// SIGKILL to each process of the unit until none is left or the limit
/// ends; the shim lives, so each orphan comes back to it. Give the count
/// left.
pub fn kill_unit(shim: u32, tag: Option<&str>, limit: Duration) -> u64 {
    kill_until_gone(|| unit_pids(shim, tag), limit)
}

/// The end of the descriptors that the shim closes with no close_range:
/// the soft limit of open files, at least 4096 and at most 2^20 (a loop
/// of one close each).
fn fallback_bound(soft: libc::rlim_t) -> i32 {
    soft.clamp(4096, 1 << 20) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::alive;
    use std::time::Instant;

    /// #281: the fallback reaches the limit of the process, not 4096.
    #[test]
    fn the_fallback_closes_to_the_limit() {
        assert_eq!(fallback_bound(65_536), 65_536);
        assert_eq!(fallback_bound(1024), 4096);
        assert_eq!(fallback_bound(0), 4096);
        assert_eq!(fallback_bound(libc::RLIM_INFINITY), 1 << 20);
    }

    fn sh(script: &str, mode: Mode) -> std::process::Child {
        let mut c = Command::new("sh");
        c.args(["-c", script]).env("PODRUNNER_UNIT_TAG", "shim-test");
        under_shim(&mut c, mode);
        c.spawn().unwrap()
    }

    /// T-703: a descriptor of the parent with no `FD_CLOEXEC` (as `flock`
    /// gives its lock to cargo) reaches no payload; the shim closes its own.
    #[test]
    fn no_descriptor_of_the_parent_reaches_the_payload() {
        use std::os::fd::AsRawFd;
        let path = std::env::temp_dir().join(format!("podrunner-shim-fd-{}", std::process::id()));
        let file = std::fs::File::create(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        // SAFETY: fcntl on a descriptor that `file` owns.
        unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, 0) };
        let mut c = Command::new("sh");
        c.args(["-c", "ls -l /proc/$$/fd"]).stdout(std::process::Stdio::piped());
        under_shim(&mut c, Mode::EndWithPayload);
        let out = c.output().unwrap();
        drop(file);
        let fds = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{fds}");
        assert!(fds.contains("/proc/") || fds.contains("pipe:") || fds.contains("/dev/"), "a list: {fds}");
        assert!(!fds.contains("podrunner-shim-fd-"), "the payload holds the descriptor:\n{fds}");
    }

    #[test]
    fn the_shim_gives_the_code_and_ends_what_is_left() {
        // An orphan with no tag and a session of its own.
        let mut c = sh("(setsid env -i sleep 300 &); exit 7", Mode::EndWithPayload);
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = c.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                let left = kill_unit(c.id(), None, Duration::from_secs(2));
                let _ = c.kill();
                let _ = c.wait();
                panic!("the shim did not end in five seconds; processes after cleanup: {left}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
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
