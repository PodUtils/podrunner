//! The directory of one test: it goes at the drop, after a pass and after
//! a panic, so a run of the tests leaves nothing in the temp directory
//! (T-591). Each test target takes it by `mod scratch;`.

// Each test target uses only a part of it.
#![allow(dead_code)]

use std::ops::Deref;
use std::path::{Path, PathBuf};
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
