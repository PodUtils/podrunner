//! The cache of image tars of a podvm driver (T-242). `prepare` saves each
//! pinned image once as `DIGEST.tar`, and the store of the units loads it
//! once (T-545). The local
//! mode keeps the cache of the user across runs, so a run does not save
//! the image again: about 300 MB of writes for each run.

use std::path::{Path, PathBuf};

use super::PodvmUnit;

/// The tars that stay in a cache of the user: the newest ones by time of
/// use; the others go.
const KEEP: usize = 4;

impl PodvmUnit {
    /// Keep the tars in `dir` (a cache of the user), not under the state of
    /// the driver. The oldest tars beyond the newest four go.
    pub fn with_image_cache(mut self, dir: &Path) -> PodvmUnit {
        prune(dir, KEEP);
        self.tars = Some(dir.to_path_buf());
        self
    }

    pub(super) fn tar_of(&self, digest: &str) -> PathBuf {
        let dir = self.tars.clone().unwrap_or_else(|| self.state_dir.join("images"));
        let tar = dir.join(format!("{}.tar", digest.trim_start_matches("sha256:")));
        // The time of use orders the tars for `prune`.
        if let Ok(f) = std::fs::File::options().append(true).open(&tar) {
            let _ = f.set_modified(std::time::SystemTime::now());
        }
        tar
    }
}

/// Remove each `*.tar` of `dir` but the `keep` newest.
fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut tars: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "tar"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    tars.sort_by_key(|a| std::cmp::Reverse(a.0));
    for (_, p) in tars.into_iter().skip(keep) {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    /// T-242: the newest tars stay.
    #[test]
    fn the_oldest_tars_go() {
        let dir = std::env::temp_dir().join(format!("podrunner-tars-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let now = std::time::SystemTime::now();
        for (i, name) in ["a.tar", "b.tar", "c.tar"].iter().enumerate() {
            let f = std::fs::File::create(dir.join(name)).unwrap();
            f.set_modified(now - std::time::Duration::from_secs(100 * (3 - i as u64))).unwrap();
        }
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        super::prune(&dir, 2);
        let mut left: Vec<String> =
            std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, ["b.tar", "c.tar", "notes.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
