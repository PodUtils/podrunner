//! The directory of the caches on the host (T-480): one directory for each
//! scope, with `data` (the unit mounts it), `scope` (the scope for the
//! operator; its time is the last use) and `bytes` (the size of `data`
//! after its last `rw` job). A job writes `data`: each walk reads the
//! status of a link and never follows it, and a cache goes whole or not at
//! all (a part of the cache of a tool, a browser, does not work). A cache
//! that goes is first renamed to a trash name in the tick, then removed
//! off the tick.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::CacheConfig;
use crate::fdwalk;

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
    /// The bytes that the job wrote, once counted: 0 for `ro` at the
    /// start, and the count of the thread after the end of a `rw` job.
    pub written: Option<u64>,
}

/// The scopes that jobs hold, shared by the tick and the thread of the
/// caches. The tick counts the holders of each scope; the thread writes
/// the `bytes` of a scope only while no other job holds it and no job
/// opened it since its walk began (a new `rw` job removes `bytes`).
#[derive(Debug, Default)]
pub struct Uses {
    held: BTreeMap<String, u32>,
    opened: BTreeMap<String, u64>,
}

impl Uses {
    /// The holders of each scope, from the jobs of the tick.
    pub fn set_held(&mut self, held: BTreeMap<String, u32>) {
        self.held = held;
    }

    /// A job takes the scope `id`, before its open.
    pub fn take(&mut self, id: &str) {
        *self.held.entry(id.to_string()).or_default() += 1;
        *self.opened.entry(id.to_string()).or_default() += 1;
    }

    /// The number of holders of `id`.
    pub fn holders(&self, id: &str) -> u32 {
        self.held.get(id).copied().unwrap_or(0)
    }

    /// The number of opens of `id`: a walk compares it before a write.
    pub fn stamp(&self, id: &str) -> u64 {
        self.opened.get(id).copied().unwrap_or(0)
    }
}

/// Write `n` as the count of the scope `id` under `root` when at most
/// `holders` jobs hold it and no job opened it since `stamp`; the lock
/// keeps a new open out until the write ends.
fn write_count(root: &Path, uses: &Mutex<Uses>, id: &str, holders: u32, stamp: u64, n: u64) {
    let u = uses.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if u.holders(id) <= holders && u.stamp(id) == stamp {
        let _ = std::fs::write(root.join(id).join(BYTES), n.to_string());
    }
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
    // that no bound walks a cache that only `ro` jobs use: a `ro` job is
    // only on a kind that proves its mount read-only.
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

/// Call `f` with the status of each regular file under `dir`, and `d` with
/// the descriptor of each directory; `f` returns false to stop. Each
/// directory opens by the descriptor of its parent with no link followed:
/// a job of the cache that runs cannot take the walk out of it (T-678).
fn walk(
    dir: &Path,
    mut f: impl FnMut(&libc::stat) -> bool,
    mut d: impl FnMut(std::os::fd::BorrowedFd<'_>),
) -> fdwalk::End {
    let opts = fdwalk::Opts { same_dev: false, max_open: fdwalk::MAX_OPEN };
    let regular = |st: &libc::stat| st.st_mode & libc::S_IFMT != libc::S_IFREG || f(st);
    fdwalk::walk(dir, opts, regular, |fd, _| d(fd)).0
}

fn len(st: &libc::stat) -> u64 {
    u64::try_from(st.st_size).unwrap_or(0)
}

/// The bytes of the files under `dir`. A walk that a job made lose its
/// place gives the most bytes: the bound treats the cache as full.
pub fn size(dir: &Path) -> u64 {
    size_with(dir, |_| {})
}

/// `size`, with `d` called with each directory before the walk reads it.
fn size_with(dir: &Path, d: impl FnMut(std::os::fd::BorrowedFd<'_>)) -> u64 {
    let mut n = 0u64;
    match walk(
        dir,
        |st| {
            n = n.saturating_add(len(st));
            true
        },
        d,
    ) {
        fdwalk::End::Lost => u64::MAX,
        _ => n,
    }
}

/// The bytes of the files under `dir`, or a count above `max`: the walk
/// stops there, and a lost walk gives one.
pub fn size_to(dir: &Path, max: u64) -> u64 {
    let mut n = 0u64;
    let count = |st: &libc::stat| {
        n = n.saturating_add(len(st));
        n <= max
    };
    match walk(dir, count, |_| {}) {
        fdwalk::End::Lost => max.saturating_add(1),
        _ => n,
    }
}

/// The bytes of the files under `dir`, and of those whose status changed
/// at or after `since`: a job cannot set the `ctime` of a file back. One
/// walk: a cache can hold many files. A lost walk gives the most bytes for
/// the first count, so the bound empties the cache.
pub fn measure(dir: &Path, since: SystemTime) -> (u64, u64) {
    let (mut all, mut new) = (0u64, 0u64);
    let count = |st: &libc::stat| {
        let (s, ns) = (u64::try_from(st.st_ctime).unwrap_or(0), u32::try_from(st.st_ctime_nsec).unwrap_or(0));
        all = all.saturating_add(len(st));
        if UNIX_EPOCH + Duration::new(s, ns) >= since {
            new = new.saturating_add(len(st));
        }
        true
    };
    match walk(dir, count, |_| {}) {
        fdwalk::End::Lost => (u64::MAX, new),
        _ => (all, new),
    }
}

/// The bytes of a cache of another job than the one that ended: `Err` with
/// `max_bytes` when a job made the walk lose its place (T-678). A cache
/// above `max_bytes` is emptied at the end of its job, so the bound counts
/// it as full, but one job cannot make the bound remove each other cache,
/// and no count is written from a lost walk.
fn other_size(data: &Path, max_bytes: u64) -> Result<u64, u64> {
    match size(data) {
        u64::MAX => Err(max_bytes),
        n => Ok(n),
    }
}

/// The count of the cache at `dir`, if it has one.
fn count_of(dir: &Path) -> Option<u64> {
    std::fs::read_to_string(dir.join(BYTES)).ok().and_then(|s| s.trim().parse().ok())
}

/// Remove the cache at `dir`. A job can take the write bit of a directory
/// away; the owner gives it back first. A cache that is gone is removed.
pub fn remove(dir: &Path) -> io::Result<()> {
    walk(dir, |_| true, crate::fdwalk::open_up_fd);
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
}

/// Each trash under `root`: a cache that a stop of `serve` left renamed.
pub fn trash(root: &Path) -> Vec<PathBuf> {
    let names = std::fs::read_dir(root).into_iter().flatten().flatten();
    names.filter(|e| e.file_name().to_string_lossy().starts_with(TRASH)).map(|e| e.path()).collect()
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
/// of `n` bytes, which the job opened at the stamp `stamp`: `just` is
/// emptied above `max_bytes`; then each cache with no use in `max_age_s`
/// goes, and the caches with the oldest use go while the directory is
/// above `total_bytes`. A cache that a job holds stays. Each cache to
/// remove, with why: the tick removes it when no job took it since.
pub fn bound(
    root: &Path,
    cfg: &CacheConfig,
    uses: &Mutex<Uses>,
    just: &str,
    stamp: u64,
    n: u64,
    now: SystemTime,
) -> Vec<(String, &'static str)> {
    let lock = || uses.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut gone = Vec::new();
    let mut take = |id: &str, why: &'static str| gone.push((id.to_string(), why));
    // The job of `just` holds it until the tick reads this result.
    let emptied = n > cfg.max_bytes && lock().holders(just) <= 1;
    if emptied {
        take(just, "above [cache] max_bytes");
    } else {
        write_count(root, uses, just, 1, stamp, n);
    }
    // `just` counts as used when a job other than its own holds it.
    let used: BTreeSet<String> = {
        let u = lock();
        let mine = |id: &str| u32::from(id == just);
        u.held.iter().filter(|(id, n)| **n > mine(id)).map(|(id, _)| id.clone()).collect()
    };
    let mut all = Vec::new();
    for e in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let id = e.file_name().to_string_lossy().into_owned();
        let ours = id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit());
        if !ours || !e.metadata().is_ok_and(|m| m.is_dir()) || used.contains(&id) || (emptied && id == just) {
            continue;
        }
        let dir = e.path();
        let last = std::fs::metadata(dir.join(SCOPE)).and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH);
        let bytes = count_of(&dir).unwrap_or_else(|| match other_size(&dir.join(DATA), cfg.max_bytes) {
            Ok(n) => {
                let stamp = lock().stamp(&id);
                write_count(root, uses, &id, 0, stamp, n);
                n
            }
            Err(n) => n,
        });
        all.push((last, id, bytes));
    }
    all.sort();
    let max_age = Duration::from_secs(cfg.max_age_s);
    let (old, mut kept): (Vec<_>, Vec<_>) =
        all.into_iter().partition(|(last, _, _)| now.duration_since(*last).is_ok_and(|age| age > max_age));
    for (_, id, _) in old {
        take(&id, "no use in [cache] max_age_s");
    }
    // The sums saturate.
    let in_use = used
        .iter()
        .map(|id| root.join(id))
        .map(|d| count_of(&d).unwrap_or_else(|| other_size(&d.join(DATA), cfg.max_bytes).unwrap_or_else(|n| n)))
        .fold(0u64, u64::saturating_add);
    let mut total = in_use.saturating_add(kept.iter().map(|(_, _, b)| *b).fold(0u64, u64::saturating_add));
    kept.reverse();
    while total > cfg.total_bytes {
        let Some((_, id, bytes)) = kept.pop() else { break };
        take(&id, "above [cache] total_bytes");
        total = total.saturating_sub(bytes);
    }
    gone
}

#[cfg(test)]
mod lost {
    use super::*;

    /// T-678: a job that moves a directory of its cache during a walk makes
    /// the walk lose its place; its own size counts as full, and the bound
    /// of another job counts it as `max_bytes`, not more.
    #[test]
    fn a_lost_walk_counts_as_full_and_as_max_bytes_for_another_job() {
        let dir = crate::tests::TempDir::new();
        let data = dir.0.join("data");
        // Deeper than the bound of descriptors, so the walk closes `a`.
        let mut p = data.join("a");
        for i in 0..fdwalk::MAX_OPEN + 2 {
            p = p.join(i.to_string());
        }
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(data.join("a/f"), [0; 9]).unwrap();
        // At the deepest directory (`data`, `a`, then each level), move
        // `a/0` out of `a`, which the walk closed.
        let (mut seen, mut moved) = (0, false);
        let n = size_with(&data, |_| {
            seen += 1;
            if seen == fdwalk::MAX_OPEN + 4 {
                moved = true;
                std::fs::rename(data.join("a/0"), data.join("moved")).unwrap();
            }
        });
        assert!(moved);
        assert_eq!(n, u64::MAX);
        assert_eq!(size(&data), 9, "a walk with no change counts the files");
        assert_eq!(other_size(&data, 50), Ok(9));
    }
}
