//! The directory of the caches on the host (T-480): one directory for each
//! scope, with `data` (the unit mounts it), `scope` (the scope for the
//! operator; its time is the last use) and `bytes` (the size of `data`
//! after its last `rw` job). A job writes `data`: each walk reads the
//! status of a link and never follows it, and a cache goes whole or not at
//! all (a part of the cache of a tool, a browser, does not work).

use std::collections::BTreeSet;
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
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

/// Make the directory of the scope `id` under `root`, write its `scope`,
/// and make each directory of `paths` under its data that crosses no link.
/// The data directory, and the time of the use.
pub fn open<'a>(
    root: &Path,
    id: &str,
    scope: &str,
    paths: impl Iterator<Item = &'a str>,
) -> io::Result<(PathBuf, SystemTime)> {
    crate::own::own_root(root)?;
    let dir = root.join(id);
    mine(&dir, 0o700)?;
    let file = dir.join(SCOPE);
    std::fs::write(&file, scope)?;
    let since = std::fs::metadata(&file)?.modified()?;
    let data = dir.join(DATA);
    mine(&data, 0o755)?;
    for p in paths {
        make_under(&data, Path::new(p));
    }
    Ok((data, since))
}

/// Make `dir` with `mode`, or check that it is a directory of this user
/// and no link.
fn mine(dir: &Path, mode: u32) -> io::Result<()> {
    // SAFETY: getuid has no failure.
    let me = unsafe { libc::getuid() };
    match std::fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() && m.uid() == me => Ok(()),
        Ok(_) => Err(io::Error::other(format!("{}: not a directory of this user", dir.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => std::fs::DirBuilder::new().mode(mode).create(dir),
        Err(e) => Err(e),
    }
}

/// Make each directory of `rel` under `base`; stop at a link or a file
/// that a job put there.
fn make_under(base: &Path, rel: &Path) {
    let mut at = base.to_path_buf();
    for c in rel.components() {
        at.push(c);
        match std::fs::symlink_metadata(&at) {
            Ok(m) if m.is_dir() => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if std::fs::DirBuilder::new().mode(0o755).create(&at).is_err() {
                    return;
                }
            }
            _ => return,
        }
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

/// The bytes of the cache at `dir` after its last `rw` job, else its size.
fn bytes_of(dir: &Path) -> u64 {
    let bytes = std::fs::read_to_string(dir.join(BYTES)).ok().and_then(|s| s.trim().parse().ok());
    bytes.unwrap_or_else(|| size(&dir.join(DATA)))
}

/// Remove the cache at `dir`. A job can take the write bit of a directory
/// away; the owner gives it back first.
fn remove(dir: &Path) -> io::Result<()> {
    walk(
        dir,
        |_| {},
        |d| {
            let _ = std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700));
        },
    );
    std::fs::remove_dir_all(dir)
}

/// The bound of the directory `root` after a `rw` job of the cache `just`
/// of `n` bytes: `just` is emptied above `max_bytes`; then each cache with no use in
/// `max_age_s` goes, and the caches with the oldest use go while the
/// directory is above `total_bytes`. A cache of `used` stays. Each cache
/// that went, with why.
pub fn bound(
    root: &Path,
    cfg: &CacheConfig,
    used: &BTreeSet<String>,
    just: &str,
    n: u64,
    now: SystemTime,
) -> Vec<(String, &'static str)> {
    let mut gone = Vec::new();
    let mut take = |id: &str, why: &'static str| {
        if remove(&root.join(id)).is_ok() {
            gone.push((id.to_string(), why));
        }
    };
    if n > cfg.max_bytes && !used.contains(just) {
        take(just, "above [cache] max_bytes");
    } else {
        let _ = std::fs::write(root.join(just).join(BYTES), n.to_string());
    }
    let mut all = Vec::new();
    for e in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let id = e.file_name().to_string_lossy().into_owned();
        let ours = id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit());
        if !ours || !e.metadata().is_ok_and(|m| m.is_dir()) || used.contains(&id) {
            continue;
        }
        let dir = e.path();
        let last = std::fs::metadata(dir.join(SCOPE)).and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH);
        all.push((last, id, bytes_of(&dir)));
    }
    all.sort();
    let max_age = Duration::from_secs(cfg.max_age_s);
    let (old, mut kept): (Vec<_>, Vec<_>) =
        all.into_iter().partition(|(last, _, _)| now.duration_since(*last).is_ok_and(|age| age > max_age));
    for (_, id, _) in old {
        take(&id, "no use in [cache] max_age_s");
    }
    let in_use: u64 = used.iter().map(|id| bytes_of(&root.join(id))).sum();
    let mut total = in_use.saturating_add(kept.iter().map(|(_, _, b)| *b).sum());
    kept.reverse();
    while total > cfg.total_bytes {
        let Some((_, id, bytes)) = kept.pop() else { break };
        take(&id, "above [cache] total_bytes");
        total = total.saturating_sub(bytes);
    }
    gone
}
