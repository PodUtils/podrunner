//! The concurrency groups across local runs (T-137): a lock file of each
//! group in the cache directory of the user (`flock`), named by the
//! repository and the group, as GitHub scopes a group to its repository. A
//! second run of a group waits for the first; with `cancel-in-progress` it
//! sends SIGTERM to the run that holds it, which cancels as at Ctrl-C.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::Duration;

use podrunner_local::Job;
use podrunner_local::concurrency::Group;
use podrunner_local::drive::{End, Executor};
use podrunner_local::exec::Live;
use podrunner_schema::JobId;

/// A held group: the lock ends with the file.
pub(super) struct Held(#[allow(dead_code)] File);

/// The file name of a group: its repository and its name, each letter that
/// a file name cannot hold as `_`.
fn file_name(repo: &str, group: &str) -> String {
    let raw = format!("{repo}__{group}");
    let s: String = raw.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '_' }).collect();
    format!("{}.lock", &s[..s.len().min(200)])
}

/// Hold `group` in `dir`; `note` tells of a wait or a cancel.
pub(super) fn hold(dir: &Path, repo: &str, group: &Group, note: &mut dyn FnMut(String)) -> Result<Held, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(file_name(repo, &group.name));
    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    // SAFETY: flock on a file descriptor that this function owns.
    let free = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    if !free {
        let mut pid = String::new();
        let _ = f.read_to_string(&mut pid);
        let pid: i32 = pid.trim().parse().unwrap_or(0);
        if group.cancel_in_progress && pid > 1 {
            note(format!("podrunner: the group {:?} cancels the run {pid} that holds it\n", group.name));
            // SAFETY: a signal to the process that wrote the lock.
            unsafe { libc::kill(pid, libc::SIGTERM) };
        } else {
            note(format!("podrunner: the group {:?} waits for the run {pid} that holds it\n", group.name));
        }
        // SAFETY: as above; this call blocks until the lock is free.
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(format!("{}: {}", path.display(), std::io::Error::last_os_error()));
        }
    }
    f.set_len(0)
        .and_then(|()| f.rewind())
        .and_then(|()| write!(f, "{}", std::process::id()))
        .map_err(|e| e.to_string())?;
    Ok(Held(f))
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
                Some(hold(&self.dir, &self.repo, &g, &mut |n| notes.lock().expect("the notes").push(n))?)
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
        let held = hold(&dir, "o/r", &g, &mut |n| notes.push(n)).unwrap();
        let path = dir.join(file_name("o/r", &g.name));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), std::process::id().to_string());
        // A second holder waits: a thread takes the lock once the first ends.
        let (d, g2) = (dir.clone(), g.clone());
        let t = std::thread::spawn(move || {
            let mut n = Vec::new();
            let h = hold(&d, "o/r", &g2, &mut |x| n.push(x)).unwrap();
            drop(h);
            n
        });
        std::thread::sleep(Duration::from_millis(300));
        assert!(!t.is_finished(), "the second holder waits");
        drop(held);
        let n = t.join().unwrap();
        assert!(n[0].contains("waits"), "{n:?}");
        assert!(notes.is_empty());
        assert_eq!(file_name("o/r", "a b/c"), "o_r__a_b_c.lock");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
