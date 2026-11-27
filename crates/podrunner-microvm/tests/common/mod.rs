//! The state directory of a live test: it goes at the drop, after a pass
//! and after a panic.

#![allow(dead_code)]

use std::ops::Deref;
use std::path::{Path, PathBuf};

pub struct Scratch(PathBuf);

impl Scratch {
    /// A new empty directory `podrunner-microvm-NAME-PID` in `TMPDIR`.
    pub fn new(name: &str) -> Scratch {
        let p = std::env::temp_dir().join(format!("podrunner-microvm-{name}-{}", std::process::id()));
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
