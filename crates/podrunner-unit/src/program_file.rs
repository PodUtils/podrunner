//! Write a program file that a process starts soon after (T-646).
//!
//! execve(2) fails with ETXTBSY while a process holds the file open for
//! write. A fork in another thread copies each descriptor of the process,
//! also one with `O_CLOEXEC`, and the child holds the copy until its own
//! exec. A rename does not help: the copy holds the inode, not the name.
//! Thus the write descriptor is opened only in a child process that forks
//! nothing. When the parent reaps the child, no process holds the file
//! open for write.

use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Write `text` to `path` with the mode 0755: in a new temp file of the
/// same directory, written and synced by a child process, then renamed
/// into place. The rename keeps a process that runs the old file, no
/// reader sees half a file, and a link at `path` is replaced, not
/// followed.
pub fn write(path: impl AsRef<Path>, text: impl AsRef<[u8]>) -> io::Result<()> {
    static N: AtomicU64 = AtomicU64::new(0);
    let path = path.as_ref();
    let name =
        path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "a path with no file name"))?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".tmp-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let tmp = path.with_file_name(tmp_name);
    let result = in_child(&tmp, text.as_ref()).and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Create `tmp` (it must not exist), set the mode 0755 on the descriptor
/// (no umask, no path to follow), write `text`, sync and close it, all in
/// a forked child. The exit code of the child is 0 or the errno.
fn in_child(tmp: &Path, text: &[u8]) -> io::Result<()> {
    let c = CString::new(tmp.as_os_str().as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: the child calls only open, fchmod, write, fsync, close and _exit,
    // which are async-signal-safe, on memory that the fork copied. It
    // allocates nothing and takes no lock.
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        // SAFETY: as above.
        unsafe { libc::_exit(child(&c, text)) };
    }
    if pid < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut status = 0;
    loop {
        // SAFETY: waitpid(2) on our own child and a local status word.
        if unsafe { libc::waitpid(pid, &mut status, 0) } == pid {
            break;
        }
        let e = io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::EINTR) {
            return Err(e);
        }
    }
    match (libc::WIFEXITED(status), libc::WEXITSTATUS(status)) {
        (true, 0) => Ok(()),
        (true, errno) => Err(io::Error::from_raw_os_error(errno)),
        _ => Err(io::Error::other("the writer of the program file was killed")),
    }
}

/// The body of the child: 0, or the errno of the first fault.
fn child(tmp: &CString, mut text: &[u8]) -> libc::c_int {
    // SAFETY: each call is async-signal-safe; errno is read at once.
    unsafe {
        let errno = || *libc::__errno_location();
        let fd = libc::open(tmp.as_ptr(), libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC, 0o600);
        if fd < 0 {
            return errno();
        }
        if libc::fchmod(fd, 0o755) != 0 {
            return errno();
        }
        while !text.is_empty() {
            let n = libc::write(fd, text.as_ptr().cast(), text.len());
            if n < 0 {
                if errno() == libc::EINTR {
                    continue;
                }
                return errno();
            }
            text = &text[n as usize..];
        }
        if libc::fsync(fd) != 0 {
            return errno();
        }
        if libc::close(fd) != 0 {
            return errno();
        }
    }
    0
}

#[cfg(test)]
#[path = "program_file_tests.rs"]
mod tests;
