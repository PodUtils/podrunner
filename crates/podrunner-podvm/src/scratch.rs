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

/// Whether the test `name` (its path in the crate) runs here: when
/// `TMPDIR` has 100 bytes or more. Else it runs once more in a child with
/// a `TMPDIR` of 100 bytes (`/tmp/pv-long-PID-tt...`), which must pass,
/// and not here (T-738).
pub(crate) fn here_or_in_long_tmp(name: &str) -> bool {
    if std::env::temp_dir().as_os_str().len() >= 100 {
        return true;
    }
    let base = format!("/tmp/pv-long-{}-", std::process::id());
    let tmp = PathBuf::from(format!("{base}{}", "t".repeat(100 - base.len())));
    std::fs::create_dir_all(&tmp).unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([name, "--exact", "--test-threads=1"])
        .env("TMPDIR", &tmp)
        .output();
    let _ = std::fs::remove_dir_all(&tmp);
    let out = out.unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success() && text.contains("1 passed"), "{name} with TMPDIR={}: {text}", tmp.display());
    false
}
