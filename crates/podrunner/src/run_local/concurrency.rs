//! The concurrency groups across local runs (T-137): a lock file of each
//! group in the cache directory of the user (`flock`), named by the
//! repository and the group, as GitHub scopes a group to its repository. A
//! second run of a group waits for the first; with `cancel-in-progress` it
//! sends SIGTERM to the run that holds it, which cancels as at Ctrl-C. As
//! on GitHub, a group keeps one waiting run: a newer waiter sends SIGTERM to
//! the older one (T-432).

use std::io::{Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use pod_core::lock::{FileLock, Mode};
use std::time::Duration;

use podrunner_local::Job;
use podrunner_local::concurrency::Group;
use podrunner_local::drive::{End, Executor};
use podrunner_local::exec::Live;
use podrunner_schema::JobId;

/// A held group: the lock ends with the file, and the group leaves the
/// groups of this process. A group that the run holds already is held by
/// its jobs with no file.
pub(super) struct Held(Option<(FileLock, PathBuf)>);

impl Drop for Held {
    fn drop(&mut self) {
        if let Some((_, path)) = &self.0 {
            HELD.lock().unwrap_or_else(|e| e.into_inner()).remove(path);
        }
    }
}

/// The groups that this process holds, and whether its run (not a job)
/// holds each one (T-417).
static HELD: std::sync::Mutex<std::collections::BTreeMap<PathBuf, bool>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// The file name of a group: its repository and its name, each letter that
/// a file name cannot hold as `_`.
fn file_name(repo: &str, group: &str) -> String {
    // GitHub compares the names of a group with no case. A readable prefix,
    // then a hash of the whole name: two groups never share a file (#222).
    let raw = format!("{repo}\n{group}").to_lowercase();
    let s: String = raw.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '_' }).collect();
    let hash: String = ring::digest::digest(&ring::digest::SHA256, raw.as_bytes()).as_ref()[..12]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{}-{hash}.lock", &s[..s.len().min(80)])
}

/// Hold `group` in `dir` for the run (`run`) or for one of its jobs; `note`
/// tells of a wait or a cancel. A job takes a group of its run at once, and
/// waits for another job of its run; a run never signals itself (T-417).
pub(super) fn hold(
    dir: &Path,
    repo: &str,
    group: &Group,
    run: bool,
    note: &mut dyn FnMut(String),
) -> Result<Held, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(file_name(repo, &group.name));
    if !run && HELD.lock().unwrap_or_else(|e| e.into_inner()).get(&path) == Some(&true) {
        return Ok(Held(None));
    }
    let fail = |e: pod_core::Failure| e.to_string();
    let lock = match FileLock::try_lock(&path, Mode::Exclusive).map_err(fail)? {
        Some(l) => l,
        None => {
            let pid = pid_of(&path);
            // The waiting place of another run, held while this run waits.
            let _waiting = match pid == std::process::id() as i32 {
                true => None,
                false => Some(wait_place(&path, group, note)?),
            };
            if pid == std::process::id() as i32 {
                note(format!("podrunner: the group {:?} waits for another job of this run\n", group.name));
            } else if group.cancel_in_progress && pid > 1 {
                note(format!("podrunner: the group {:?} cancels the run {pid} that holds it\n", group.name));
                // SAFETY: a signal to the process that wrote the lock.
                unsafe { libc::kill(pid, libc::SIGTERM) };
            } else {
                note(format!("podrunner: the group {:?} waits for the run {pid} that holds it\n", group.name));
            }
            // A signal (a newer run of the group, or Ctrl-C) ends the wait.
            FileLock::lock_until(&path, Mode::Exclusive, stopped).map_err(|_| interrupted(group))?
        }
    };
    write_pid(&lock)?;
    HELD.lock().unwrap_or_else(|e| e.into_inner()).insert(path.clone(), run);
    Ok(Held(Some((lock, path))))
}

/// The process id in a lock file of a group, read with no lock.
fn pid_of(path: &Path) -> i32 {
    std::fs::read_to_string(path).ok().and_then(|p| p.trim().parse().ok()).unwrap_or(0)
}

/// Write the id of this process into the held lock file.
fn write_pid(lock: &FileLock) -> Result<(), String> {
    let mut f = lock.file();
    f.set_len(0).and_then(|()| f.rewind()).and_then(|()| write!(f, "{}", std::process::id())).map_err(|e| e.to_string())
}

/// A signal of the run: the handler of `cancel` sets the flag.
fn stopped() -> bool {
    podrunner_local::cancel::SIGNALLED.load(Ordering::SeqCst)
}

fn interrupted(group: &Group) -> String {
    format!(
        "the wait for the group {:?} ended: a signal (a newer run of the group took the waiting place, or Ctrl-C)",
        group.name
    )
}

/// Take the waiting place of the group of `path`: the run that waits there
/// gets SIGTERM, as GitHub cancels a pending run when a newer one comes
/// (T-432). The place holds until the file goes.
fn wait_place(path: &Path, group: &Group, note: &mut dyn FnMut(String)) -> Result<FileLock, String> {
    let place = path.with_extension("waiting");
    let lock = match FileLock::try_lock(&place, Mode::Exclusive).map_err(|e| e.to_string())? {
        Some(l) => l,
        None => {
            let pid = pid_of(&place);
            if pid > 1 && pid != std::process::id() as i32 {
                note(format!(
                    "podrunner: the group {:?} cancels the waiting run {pid}: a group keeps one waiting run\n",
                    group.name
                ));
                // SAFETY: a signal to the process that wrote the waiting place.
                unsafe { libc::kill(pid, libc::SIGTERM) };
            }
            FileLock::lock_until(&place, Mode::Exclusive, stopped).map_err(|_| interrupted(group))?
        }
    };
    write_pid(&lock)?;
    Ok(lock)
}

/// The executor that holds the group of each job while it runs.
pub(super) struct Grouped {
    pub inner: Box<dyn Executor>,
    pub dir: std::path::PathBuf,
    pub repo: String,
    pub next: Option<Group>,
    pub notes: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl Executor for Grouped {
    fn concurrency(&mut self, group: Option<&Group>) {
        self.next = group.cloned();
    }
    fn select(&mut self, labels: &[String]) -> Result<(), String> {
        self.inner.select(labels)
    }
    fn environment(&mut self, name: Option<&str>) {
        self.inner.environment(name)
    }
    fn set_live(&mut self, live: Live) {
        self.inner.set_live(live)
    }
    fn set_stop(&mut self, stop: podrunner_local::cancel::Stop) {
        self.inner.set_stop(stop)
    }
    fn take_logs(&mut self) -> Vec<(JobId, String)> {
        self.inner.take_logs()
    }
    fn take_blobs(&mut self) -> Vec<(JobId, Vec<(String, String)>)> {
        self.inner.take_blobs()
    }
    fn run(&mut self, job: &JobId, message: Job, limit: Duration) -> Result<End, String> {
        let _held = match self.next.take() {
            Some(g) => {
                let notes = self.notes.clone();
                Some(hold(&self.dir, &self.repo, &g, false, &mut |n| notes.lock().expect("the notes").push(n))?)
            }
            None => None,
        };
        self.inner.run(job, message, limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_group_makes_the_next_wait_or_cancel() {
        let dir = std::env::temp_dir().join(format!("podrunner-group-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let g = Group { name: "deploy-refs/heads/main".into(), cancel_in_progress: false };
        let mut notes = Vec::new();
        let held = hold(&dir, "o/r", &g, false, &mut |n| notes.push(n)).unwrap();
        let path = dir.join(file_name("o/r", &g.name));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), std::process::id().to_string());
        // A second holder waits: a thread takes the lock once the first ends.
        let (d, g2) = (dir.clone(), g.clone());
        let t = std::thread::spawn(move || {
            let mut n = Vec::new();
            let h = hold(&d, "o/r", &g2, false, &mut |x| n.push(x)).unwrap();
            drop(h);
            n
        });
        std::thread::sleep(Duration::from_millis(300));
        assert!(!t.is_finished(), "the second holder waits");
        drop(held);
        let n = t.join().unwrap();
        assert!(n[0].contains("waits"), "{n:?}");
        assert!(notes.is_empty());
        // #222: distinct groups get distinct files; the case does not count.
        let name = file_name("o/r", "a b/c");
        assert!(name.starts_with("o_r_a_b_c-") && name.ends_with(".lock"), "{name}");
        assert_ne!(file_name("o/r", "a b"), file_name("o/r", "a_b"));
        let (long1, long2) = ("x".repeat(300) + "1", "x".repeat(300) + "2");
        assert_ne!(file_name("o/r", &long1), file_name("o/r", &long2));
        assert!(file_name("o/r", &long1).len() < 120);
        assert_eq!(file_name("O/R", "Prod"), file_name("o/r", "prod"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-417: a job takes a group that its run holds at once, and a job
    /// that meets another job of its run waits for it: it never sends a
    /// signal to its own run, also with `cancel-in-progress`.
    #[test]
    fn a_run_never_waits_for_or_cancels_itself() {
        let dir = std::env::temp_dir().join(format!("podrunner-group-self-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let g = Group { name: "self".into(), cancel_in_progress: true };
        let run = hold(&dir, "o/r", &g, true, &mut |_| {}).unwrap();
        let mut notes = Vec::new();
        let start = std::time::Instant::now();
        let job = hold(&dir, "o/r", &g, false, &mut |n| notes.push(n)).unwrap();
        assert!(start.elapsed() < Duration::from_secs(1), "the job of the run takes its group at once");
        drop((job, run));
        // Two jobs of one run: the second waits for the first.
        let g2 = Group { name: "cells".into(), cancel_in_progress: true };
        let first = hold(&dir, "o/r", &g2, false, &mut |_| {}).unwrap();
        let (d, g3) = (dir.clone(), g2.clone());
        let t = std::thread::spawn(move || {
            let mut n = Vec::new();
            drop(hold(&d, "o/r", &g3, false, &mut |x| n.push(x)).unwrap());
            n
        });
        std::thread::sleep(Duration::from_millis(300));
        assert!(!t.is_finished(), "the second job waits");
        drop(first);
        let n = t.join().unwrap();
        assert!(n.iter().any(|l| l.contains("another job of this run")), "{n:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-432: a group keeps one waiting run; a newer waiter sends SIGTERM to
    /// the older one. Two processes hold the group and its waiting place.
    #[test]
    fn a_newer_waiter_cancels_the_older_one() {
        let dir = std::env::temp_dir().join(format!("podrunner-group-wait-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let g = Group { name: "deploy".into(), cancel_in_progress: false };
        let path = dir.join(file_name("o/r", &g.name));
        // A process holds each file, and writes its pid in it.
        let take = |file: &Path| {
            let script = format!("echo $$ > '{}'; exec flock -o '{}' sleep 300", file.display(), file.display());
            let c = std::process::Command::new("sh").args(["-c", &script]).spawn().unwrap();
            while std::fs::read_to_string(file).map_or(true, |t| t.trim().is_empty()) {
                std::thread::sleep(Duration::from_millis(20));
            }
            std::thread::sleep(Duration::from_millis(200));
            c
        };
        let mut holder = take(&path);
        let mut older = take(&path.with_extension("waiting"));
        let (d, g2) = (dir.clone(), g.clone());
        let t = std::thread::spawn(move || {
            let mut n = Vec::new();
            drop(hold(&d, "o/r", &g2, true, &mut |x| n.push(x)).unwrap());
            n
        });
        // The older waiter ends by the signal; the new one waits on.
        let status = older.wait().unwrap();
        assert!(!status.success(), "{status:?}");
        std::thread::sleep(Duration::from_millis(300));
        assert!(!t.is_finished(), "the new waiter waits for the holder");
        let _ = holder.kill();
        let _ = holder.wait();
        let n = t.join().unwrap();
        assert!(n.iter().any(|l| l.contains("cancels the waiting run")), "{n:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
