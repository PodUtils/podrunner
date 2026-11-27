//! The directory of one test: it goes at the drop, after a pass and after
//! a panic, so a run of the tests leaves nothing in the temp directory
//! (T-591).

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) struct Scratch(PathBuf);

impl Scratch {
    /// A new empty directory `podrunner-NAME-PID-N` in the temp directory
    /// (`TMPDIR`).
    pub(crate) fn new(name: &str) -> Scratch {
        Scratch::under(&std::env::temp_dir(), &format!("podrunner-{name}"))
    }

    /// A new empty directory `/tmp/pv-NAME-PID-N` for Unix sockets:
    /// `sun_path` holds 107 bytes, and a long `TMPDIR` passes that limit
    /// (T-735).
    pub(crate) fn short(name: &str) -> Scratch {
        Scratch::under(Path::new("/tmp"), &format!("pv-{name}"))
    }

    fn under(base: &Path, name: &str) -> Scratch {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let p = base.join(format!("{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Scratch(p)
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
