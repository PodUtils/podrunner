//! A walk of a tree that a running job can change (T-678). Each directory
//! is opened relative to the descriptor of its parent, with no link
//! followed, so a component that a job swaps for a link between two reads
//! never takes the walk out of the tree. The walk holds at most
//! `max_open` descriptors of directories: above that, it closes the
//! oldest, and opens it again through `..` of its child, checked by its
//! device and inode. When that check fails (a job moved the directory),
//! the walk ends `Lost`, and the caller fails closed.

use std::ffi::{CStr, CString};
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// How a walk ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    /// Each directory that the walk could open was read.
    Done,
    /// The file callback asked to stop.
    Stopped,
    /// A directory moved while the walk held no descriptor of it.
    Lost,
}

/// The options of a walk.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Opts {
    /// Skip each entry on another device than the root (a mount point).
    pub same_dev: bool,
    /// The most descriptors of directories that the walk holds open.
    pub max_open: usize,
}

/// The descriptors that a walk holds at most, unless the caller says
/// otherwise: a deep tree of a job must not take the descriptors of serve.
pub(crate) const MAX_OPEN: usize = 64;

/// One directory of the path from the root to the directory read last.
struct Level {
    /// The readable descriptor, or none after the walk closed it.
    fd: Option<OwnedFd>,
    dev: u64,
    ino: u64,
    /// The names of the subdirectories that the walk has not opened yet.
    pending: Vec<CString>,
}

fn fstat(fd: &impl AsRawFd) -> Option<libc::stat> {
    let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: a valid fd and a buffer of the size of `stat`.
    (unsafe { libc::fstat(fd.as_raw_fd(), st.as_mut_ptr()) } == 0).then(|| unsafe { st.assume_init() })
}

fn openat(at: libc::c_int, name: &CStr, flags: libc::c_int) -> Option<OwnedFd> {
    // SAFETY: a valid fd or AT_FDCWD, and a C string.
    let fd = unsafe { libc::openat(at, name.as_ptr(), flags | libc::O_CLOEXEC) };
    // SAFETY: openat gave a new fd that nothing else owns.
    (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) })
}

const PATH: libc::c_int = libc::O_PATH | libc::O_DIRECTORY | libc::O_NOFOLLOW;
const READ: libc::c_int = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW;

/// The names in the directory of `fd`, with no `.` and `..`.
fn names(fd: &OwnedFd) -> Vec<CString> {
    // fdopendir takes its fd: give it a second one.
    // SAFETY: a valid fd.
    let dup = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
    if dup < 0 {
        return Vec::new();
    }
    // SAFETY: a valid fd of a directory, owned by the stream from here.
    let dir = unsafe { libc::fdopendir(dup) };
    if dir.is_null() {
        // SAFETY: fdopendir failed, so the fd is still ours.
        unsafe { libc::close(dup) };
        return Vec::new();
    }
    let mut out = Vec::new();
    loop {
        // SAFETY: a valid stream.
        let e = unsafe { libc::readdir(dir) };
        if e.is_null() {
            break;
        }
        // SAFETY: readdir gave an entry with a NUL-terminated name.
        let name = unsafe { CStr::from_ptr((*e).d_name.as_ptr()) };
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            out.push(name.to_owned());
        }
    }
    // SAFETY: a valid stream, closed once; it closes `dup`.
    unsafe { libc::closedir(dir) };
    out
}

/// Walk the tree under `root`. `dir` gets the `O_PATH` descriptor of each
/// directory (the root too) before the walk reads it, with its name; `file`
/// gets the status of each entry that is not a directory and returns false
/// to stop. No link is followed. The walk holds at most `max_open` open
/// levels, and gives their peak in the second value; with the descriptors
/// of one open and of one read, it holds at most `max_open + 2`.
pub(crate) fn walk(
    root: &Path,
    opts: Opts,
    mut file: impl FnMut(&libc::stat) -> bool,
    mut dir: impl FnMut(BorrowedFd<'_>, &CStr),
) -> (End, usize) {
    let max_open = opts.max_open.max(1);
    let Ok(c) = CString::new(root.as_os_str().as_bytes()) else { return (End::Done, 0) };
    let Some((first, dev, ino)) = enter(libc::AT_FDCWD, &c, None, &mut dir) else { return (End::Done, 0) };
    let mut levels = Vec::new();
    let (mut open, mut peak) = (0usize, 0usize);
    let mut todo = Some((first, dev, ino));
    loop {
        if let Some((fd, d, i)) = todo.take() {
            let mut pending = Vec::new();
            for name in names(&fd) {
                let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
                // SAFETY: a valid fd, a C string and a buffer of the size of `stat`.
                let ok =
                    unsafe { libc::fstatat(fd.as_raw_fd(), name.as_ptr(), st.as_mut_ptr(), libc::AT_SYMLINK_NOFOLLOW) };
                if ok != 0 {
                    continue;
                }
                // SAFETY: fstatat filled it.
                let st = unsafe { st.assume_init() };
                if opts.same_dev && st.st_dev != dev {
                    continue;
                }
                if st.st_mode & libc::S_IFMT == libc::S_IFDIR {
                    pending.push(name);
                } else if !file(&st) {
                    return (End::Stopped, peak);
                }
            }
            open += 1;
            peak = peak.max(open);
            levels.push(Level { fd: Some(fd), dev: d, ino: i, pending });
            // Close the oldest open directory above the bound; the top stays.
            if open > max_open
                && let Some(old) = levels.iter_mut().rev().skip(1).filter(|l| l.fd.is_some()).last()
            {
                old.fd = None;
                open -= 1;
            }
        }
        let Some(top) = levels.last_mut() else { return (End::Done, peak) };
        if let Some(name) = top.pending.pop() {
            let at = top.fd.as_ref().expect("the top directory is open").as_raw_fd();
            let same = opts.same_dev.then_some(dev);
            todo = enter(at, &name, same, &mut dir);
            continue;
        }
        // The top is done: open its parent again when the walk closed it.
        let child = levels.pop().expect("a top level");
        open -= 1;
        let Some(parent) = levels.last_mut() else { return (End::Done, peak) };
        if parent.fd.is_none() {
            let up = child.fd.as_ref().and_then(|fd| openat(fd.as_raw_fd(), c"..", READ));
            match up.and_then(|fd| fstat(&fd).map(|st| (fd, st))) {
                Some((fd, st)) if st.st_dev == parent.dev && st.st_ino == parent.ino => {
                    parent.fd = Some(fd);
                    open += 1;
                    peak = peak.max(open);
                }
                _ => return (End::Lost, peak),
            }
        }
        drop(child);
    }
}

/// Open the directory `name` under `at` with no link followed, give its
/// `O_PATH` descriptor to `dir`, then open it to read. Its device and
/// inode come with it. `same`: skip a directory on another device.
fn enter(
    at: libc::c_int,
    name: &CStr,
    same: Option<u64>,
    dir: &mut impl FnMut(BorrowedFd<'_>, &CStr),
) -> Option<(OwnedFd, u64, u64)> {
    let path = openat(at, name, PATH)?;
    let st = fstat(&path)?;
    if same.is_some_and(|d| d != st.st_dev) {
        return None;
    }
    // SAFETY: `path` is open for the length of the call.
    dir(unsafe { BorrowedFd::borrow_raw(path.as_raw_fd()) }, name);
    let fd = openat(path.as_raw_fd(), c".", READ)?;
    let again = fstat(&fd)?;
    // `.` of the descriptor is the directory that it names.
    (again.st_dev == st.st_dev && again.st_ino == st.st_ino).then_some((fd, st.st_dev, st.st_ino))
}

/// Give the owner the bits of the directory of the `O_PATH` descriptor
/// `fd` again: by `fchmod` of a descriptor opened for read, and when the
/// directory has no read bit, by chmod of its link in /proc, which names
/// the directory itself, so a job cannot point it at another file.
pub(crate) fn open_up_fd(fd: BorrowedFd<'_>) {
    if let Some(dir) = openat(fd.as_raw_fd(), c".", READ) {
        // SAFETY: a valid fd.
        if unsafe { libc::fchmod(dir.as_raw_fd(), 0o700) } == 0 {
            return;
        }
    }
    let Ok(p) = CString::new(format!("/proc/self/fd/{}", fd.as_raw_fd())) else { return };
    // SAFETY: a C string.
    unsafe { libc::chmod(p.as_ptr(), 0o700) };
}

#[cfg(test)]
mod tests;
