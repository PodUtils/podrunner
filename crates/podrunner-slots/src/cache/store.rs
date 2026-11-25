//! The directory of the caches on the host (T-480): one directory for each
//! scope, with `data` (the unit mounts it), `scope` (the scope for the
//! operator; its time is the last use) and `bytes` (the size of `data`
//! after its last `rw` job). A job writes `data`: each walk reads the
//! status of a link and never follows it, and a cache goes whole or not at
//! all (a part of the cache of a tool, a browser, does not work). A cache
//! that goes is first renamed to a trash name in the tick, then removed
//! off the tick.

use std::collections::BTreeSet;
use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::CacheConfig;

/// How a unit sees its cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Ro,
    Rw,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Ro => "ro",
            Mode::Rw => "rw",
        }
    }
}

/// The cache of one job.
#[derive(Debug, Clone)]
pub struct Grant {
    /// The id of its scope: 32 hex digits.
    pub id: String,
    /// The directory of the caches.
    pub root: PathBuf,
    pub data: PathBuf,
    pub mode: Mode,
    /// The time of the file `scope` at the start: a file whose status
    /// changed at or after it was written in the run. The clock of the
    /// files is coarser than the clock of the process.
    pub since: SystemTime,
    /// The bytes that the job wrote, after its end.
    pub written: u64,
}

const DATA: &str = "data";
const SCOPE: &str = "scope";
const BYTES: &str = "bytes";
const LOCK: &str = "lock";
/// The start of the name of a cache that goes.
const TRASH: &str = "trash-";

/// Hold the directory of the caches for this process: a second `serve` on
/// it would remove a cache that a job of the first one uses. The lock
/// ends with the file.
pub fn lock(root: &Path) -> io::Result<std::fs::File> {
    crate::own::own_root(root)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root.join(LOCK))?;
    // SAFETY: a valid fd of the file.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let e = io::Error::last_os_error();
        return Err(io::Error::other(format!("{}: in use by another serve ({e})", root.display())));
    }
    Ok(file)
}

/// Make the directory of the scope `id` under `root` for a job of `mode`,
/// write its `scope`, and make each directory of `paths` under its data
/// that crosses no link. The data directory, and the time of the use.
pub fn open<'a>(
    root: &Path,
    id: &str,
    scope: &str,
    mode: Mode,
    paths: impl Iterator<Item = &'a str>,
) -> io::Result<(PathBuf, SystemTime)> {
    crate::own::own_root(root)?;
    let dir = root.join(id);
    mine(&dir, 0o700)?;
    let file = dir.join(SCOPE);
    std::fs::write(&file, scope)?;
    let since = std::fs::metadata(&file)?.modified()?;
    let data = dir.join(DATA);
    let made = mine(&data, 0o755)?;
    // A `rw` job changes the count: with none, a bound walks the cache, also
    // after a stop in the job. A new cache of a `ro` job has its count, so
    // that no bound walks a cache that only `ro` jobs use.
    if mode == Mode::Rw {
        match std::fs::remove_file(dir.join(BYTES)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    } else if made {
        std::fs::write(dir.join(BYTES), "0")?;
    }
    for p in paths {
        make_under(&data, Path::new(p));
    }
    Ok((data, since))
}

/// Make `dir` with `mode`, or check that it is a directory of this user
/// and no link. True when it made it.
fn mine(dir: &Path, mode: u32) -> io::Result<bool> {
    // SAFETY: getuid has no failure.
    let me = unsafe { libc::getuid() };
    match std::fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() && m.uid() == me => Ok(false),
        Ok(_) => Err(io::Error::other(format!("{}: not a directory of this user", dir.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new().mode(mode).create(dir).map(|()| true)
        }
        Err(e) => Err(e),
    }
}

/// Open the directory `name` under `at` (none: the path `name`) and no
/// link.
fn open_dir(at: Option<&OwnedFd>, name: &[u8]) -> Option<OwnedFd> {
    let name = CString::new(name).ok()?;
    let at = at.map_or(libc::AT_FDCWD, AsRawFd::as_raw_fd);
    let flags = libc::O_PATH | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    // SAFETY: a valid fd or AT_FDCWD, and a C string.
    let fd = unsafe { libc::openat(at, name.as_ptr(), flags) };
    // SAFETY: openat gave a new fd that nothing else owns.
    (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Make each directory of `rel` under `base`; stop at a link or a file
/// that a job put there. Each step is relative to the fd of the last
/// directory: a job of the cache that runs cannot swap a part for a link
/// between the check and the use.
fn make_under(base: &Path, rel: &Path) {
    let Some(mut dir) = open_dir(None, base.as_os_str().as_bytes()) else { return };
    for c in rel.components() {
        let Component::Normal(name) = c else { return };
        let Ok(cname) = CString::new(name.as_bytes()) else { return };
        // SAFETY: a valid fd and a C string.
        if unsafe { libc::mkdirat(dir.as_raw_fd(), cname.as_ptr(), 0o755) } != 0
            && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists
        {
            return;
        }
        let Some(next) = open_dir(Some(&dir), name.as_bytes()) else { return };
        dir = next;
    }
}

/// Call `f` with each file under `dir`, and `d` with each directory. No
/// link is followed; a deep tree needs no deep stack.
fn walk(dir: &Path, mut f: impl FnMut(&std::fs::Metadata), mut d: impl FnMut(&Path)) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        d(&at);
        let Ok(entries) = std::fs::read_dir(&at) else { continue };
        for e in entries.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                stack.push(e.path());
            } else if m.is_file() {
                f(&m);
            }
        }
    }
}

/// The bytes of the files under `dir`.
pub fn size(dir: &Path) -> u64 {
    let mut n = 0u64;
    walk(dir, |m| n = n.saturating_add(m.len()), |_| {});
    n
}

/// The bytes of the files under `dir`, or a count above `max`: the walk
/// stops there.
pub fn size_to(dir: &Path, max: u64) -> u64 {
    let mut n = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else { continue };
        for e in entries.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                stack.push(e.path());
            } else if m.is_file() {
                n = n.saturating_add(m.len());
                if n > max {
                    return n;
                }
            }
        }
    }
    n
}

/// The bytes of the files under `dir`, and of those whose status changed
/// at or after `since`: a job cannot set the `ctime` of a file back. One
/// walk: a cache can hold many files.
pub fn measure(dir: &Path, since: SystemTime) -> (u64, u64) {
    let (mut all, mut new) = (0u64, 0u64);
    let changed = |m: &std::fs::Metadata| {
        let (s, ns) = (u64::try_from(m.ctime()).unwrap_or(0), u32::try_from(m.ctime_nsec()).unwrap_or(0));
        UNIX_EPOCH + Duration::new(s, ns)
    };
    let count = |m: &std::fs::Metadata| {
        all = all.saturating_add(m.len());
        if changed(m) >= since {
            new = new.saturating_add(m.len());
        }
    };
    walk(dir, count, |_| {});
    (all, new)
}

/// The bytes of the cache at `dir` after its last `rw` job, else its size,
/// which is kept when `keep` (no job uses it).
fn bytes_of(dir: &Path, keep: bool) -> u64 {
    let bytes = std::fs::read_to_string(dir.join(BYTES)).ok().and_then(|s| s.trim().parse().ok());
    bytes.unwrap_or_else(|| {
        let n = size(&dir.join(DATA));
        if keep {
            let _ = std::fs::write(dir.join(BYTES), n.to_string());
        }
        n
    })
}

/// Give the owner the bits of the directory `d` again, and follow no link
/// at `d`.
pub fn open_up(d: &Path) {
    let Ok(c) = CString::new(d.as_os_str().as_bytes()) else { return };
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    // SAFETY: a C string; the fd is closed below.
    let fd = unsafe { libc::open(c.as_ptr(), flags) };
    if fd >= 0 {
        // SAFETY: a valid fd of a directory, closed once.
        unsafe {
            libc::fchmod(fd, 0o700);
            libc::close(fd);
        }
        return;
    }
    // A directory with no read bit cannot be opened: chmod it by its name,
    // with no link followed.
    // SAFETY: a C string.
    unsafe { libc::fchmodat(libc::AT_FDCWD, c.as_ptr(), 0o700, libc::AT_SYMLINK_NOFOLLOW) };
}

/// Remove the cache at `dir`. A job can take the write bit of a directory
/// away; the owner gives it back first.
pub fn remove(dir: &Path) -> io::Result<()> {
    walk(dir, |_| {}, open_up);
    std::fs::remove_dir_all(dir)
}

/// Rename the cache `id` under `root` to a trash name, for a removal off
/// the tick. A rename takes no walk.
pub fn evict(root: &Path, id: &str) -> io::Result<PathBuf> {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let trash = root.join(format!("{TRASH}{id}-{n}"));
    std::fs::rename(root.join(id), &trash)?;
    Ok(trash)
}

/// The bound of the directory `root` after a `rw` job of the cache `just`
/// of `n` bytes: `just` is emptied above `max_bytes`; then each cache with
/// no use in `max_age_s` goes, and the caches with the oldest use go while
/// the directory is above `total_bytes`. A cache of `used` stays. Each
/// cache to remove, with why: the tick removes it when no job took it
/// since. A trash that a stop left is removed here.
pub fn bound(
    root: &Path,
    cfg: &CacheConfig,
    used: &BTreeSet<String>,
    just: &str,
    n: u64,
    now: SystemTime,
) -> Vec<(String, &'static str)> {
    let mut gone = Vec::new();
    let mut take = |id: &str, why: &'static str| gone.push((id.to_string(), why));
    if n > cfg.max_bytes && !used.contains(just) {
        take(just, "above [cache] max_bytes");
    } else {
        let _ = std::fs::write(root.join(just).join(BYTES), n.to_string());
    }
    let mut all = Vec::new();
    for e in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let id = e.file_name().to_string_lossy().into_owned();
        if id.starts_with(TRASH) {
            let _ = remove(&e.path());
            continue;
        }
        let ours = id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit());
        if !ours || !e.metadata().is_ok_and(|m| m.is_dir()) || used.contains(&id) {
            continue;
        }
        let dir = e.path();
        let last = std::fs::metadata(dir.join(SCOPE)).and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH);
        all.push((last, id, bytes_of(&dir, true)));
    }
    all.sort();
    let max_age = Duration::from_secs(cfg.max_age_s);
    let (old, mut kept): (Vec<_>, Vec<_>) =
        all.into_iter().partition(|(last, _, _)| now.duration_since(*last).is_ok_and(|age| age > max_age));
    for (_, id, _) in old {
        take(&id, "no use in [cache] max_age_s");
    }
    let in_use: u64 = used.iter().map(|id| bytes_of(&root.join(id), false)).sum();
    let mut total = in_use.saturating_add(kept.iter().map(|(_, _, b)| *b).sum());
    kept.reverse();
    while total > cfg.total_bytes {
        let Some((_, id, bytes)) = kept.pop() else { break };
        take(&id, "above [cache] total_bytes");
        total = total.saturating_sub(bytes);
    }
    gone
}
