//! The directory of one test: it goes at the drop, after a pass and after
//! a panic, so a run of the tests leaves nothing in the temp directory
//! (T-591). Each test target takes it by `mod scratch;`.

// Each test target uses only a part of it.
#![allow(dead_code)]

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct Scratch(PathBuf);

impl Scratch {
    /// A new empty directory `podrunner-NAME-PID-N` in the temp directory
    /// (`TMPDIR`).
    pub fn new(name: &str) -> Scratch {
        Scratch::under(&std::env::temp_dir(), &format!("podrunner-{name}"))
    }

    /// A new empty directory `/tmp/pr-NAME-PID-N` for Unix sockets:
    /// `sun_path` holds 107 bytes, and a long `TMPDIR` passes that limit
    /// (T-735).
    pub fn short(name: &str) -> Scratch {
        Scratch::under(Path::new("/tmp"), &format!("pr-{name}"))
    }

    fn under(base: &Path, name: &str) -> Scratch {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let p = base.join(format!("{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Scratch(p)
    }

    /// The path of the socket `name` in the directory. It fails at once,
    /// with the path, when `sun_path` cannot hold it.
    pub fn sock(&self, name: &str) -> PathBuf {
        let p = self.0.join(name);
        let n = p.as_os_str().len();
        assert!(n < 108, "the socket path has {n} bytes, over the 107 of sun_path: {}", p.display());
        p
    }
}

impl Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A child that gets SIGKILL and a wait at the drop: a failed assertion
/// leaves no `podrunner serve` that holds a descriptor of the test run
/// (T-739). Each test that starts a serve takes `spawn_killed`.
pub struct Killed(Option<Child>);

impl Killed {
    /// Wait for the end of the child and give its output, as
    /// `Child::wait_with_output` does.
    pub fn wait_with_output(mut self) -> std::io::Result<Output> {
        self.0.take().expect("the child is there until the drop").wait_with_output()
    }
}

impl Deref for Killed {
    type Target = Child;
    fn deref(&self) -> &Child {
        self.0.as_ref().expect("the child is there until the drop")
    }
}

impl DerefMut for Killed {
    fn deref_mut(&mut self) -> &mut Child {
        self.0.as_mut().expect("the child is there until the drop")
    }
}

impl Drop for Killed {
    fn drop(&mut self) {
        if let Some(c) = self.0.as_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

pub trait SpawnKilled {
    /// Start the command with a `Killed` guard on the child.
    fn spawn_killed(&mut self) -> Killed;
}

impl SpawnKilled for Command {
    fn spawn_killed(&mut self) -> Killed {
        Killed(Some(self.spawn().unwrap()))
    }
}
