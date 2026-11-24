//! The cache of image tars of a podvm driver (T-242). `prepare` saves each
//! pinned image once as `DIGEST.tar`, and the store of the units loads it
//! once (T-545). The local
//! mode keeps the cache of the user across runs, so a run does not save
//! the image again: about 300 MB of writes for each run. A save writes a
//! temporary name and renames it at its end; a tar that does not load and
//! is not whole goes, and the start saves it again once (T-630).

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use podrunner_unit::UnitError;

use super::{PodvmUnit, host};

/// The tars that stay in a cache of the user: the newest ones by time of
/// use; the others go.
const KEEP: usize = 4;

/// A temporary tar of a save that is older than this is of a save that
/// stopped: a save writes its file the whole time.
const STALE_PART: std::time::Duration = std::time::Duration::from_secs(3600);

/// The file name of the tar of `digest` (`sha256:HEX`) in a cache.
pub fn tar_name(digest: &str) -> String {
    format!("{}.tar", digest.trim_start_matches("sha256:"))
}

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
        let tar = dir.join(tar_name(digest));
        // The time of use orders the tars for `prune`.
        if let Ok(f) = std::fs::File::options().append(true).open(&tar) {
            let _ = f.set_modified(std::time::SystemTime::now());
        }
        tar
    }

    /// Save `reference` of the store of the images as `tar`. The save
    /// writes a temporary name, so a cut save (a signal, a full disk)
    /// leaves no tar that a later run takes.
    pub(super) fn save_tar(&self, reference: &str, platform: &str, tar: &Path) -> Result<(), UnitError> {
        fs::create_dir_all(tar.parent().expect("a directory")).map_err(|e| host("the image cache", e))?;
        let part = tar.with_extension(format!("part-{}", std::process::id()));
        let save = |u: &PodvmUnit| u.podvm(&u.images, &["save", "-o", &part.to_string_lossy(), reference], None);
        let saved = save(self).or_else(|first| {
            // An image that a digest pins and that the store does not
            // have yet (a `container:` job of `serve`, T-177): pull it
            // once by its digest, then save it.
            self.podvm(&self.images, &["pull", "-q", "--platform", platform, reference], None)
                .map_err(|e| UnitError::new(e.code, format!("{}; then podvm pull: {}", first.detail, e.detail)))?;
            save(self)
        });
        if let Err(e) = saved {
            let _ = fs::remove_file(&part);
            return Err(e);
        }
        fs::rename(&part, tar).map_err(|e| host("the image cache", e))
    }

    /// The reference of the image of `digest` in `store`, loaded from its
    /// tar in the cache. When the load fails and the tar is not whole (a
    /// cut save, a full disk, a change on the disk), the tar goes and the
    /// image is saved and loaded again, once.
    pub(super) fn cached(&self, store: &Path, digest: &str) -> Result<String, UnitError> {
        let tar = self.tar_of(digest);
        let before = file_id(&tar);
        let first = match self.loaded(store, &tar) {
            Ok(r) => return Ok(r),
            Err(e) => e,
        };
        // Runs that share the cache repair a tar one at a time, and a run
        // never removes the tar that another run has just saved.
        let _lock = lock(&tar);
        if file_id(&tar) != before && tar_fault(&tar).is_none() {
            return self.loaded(store, &tar);
        }
        let Some(fault) = tar_fault(&tar) else { return Err(first) };
        let named = |detail: String| {
            let t = tar.display();
            UnitError::new(
                first.code,
                format!("{detail}; the tar {t} of the image cache is not whole ({fault}); fix: rm {t}"),
            )
        };
        let reference = self.saved.lock().expect("the saved images").get(digest).cloned();
        let Some(reference) = reference else { return Err(named(first.detail.clone())) };
        let _ = fs::remove_file(&tar);
        let platform = self.platform.clone();
        self.save_tar(&reference, &platform, &tar)
            .and_then(|()| self.loaded(store, &tar))
            .map_err(|e| named(format!("{}; it was saved again, and: {}", first.detail, e.detail)))
    }
}

/// The device and the inode of `path`: a save again gives a new inode.
fn file_id(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).ok().map(|m| (m.dev(), m.ino()))
}

/// An exclusive lock on `DIGEST.lock` beside `tar`, held until the file
/// drops. With no lock (a file system that refuses it), the repair runs
/// with no lock.
fn lock(tar: &Path) -> Option<fs::File> {
    let f = fs::File::options().create(true).truncate(false).write(true).open(tar.with_extension("lock")).ok()?;
    f.lock().ok()?;
    Some(f)
}

/// Why `tar` is not a whole image tar: it does not read to its end, it has
/// no `index.json`, a blob has a SHA-256 other than its name, or a blob
/// that the index or the manifest names is not in it. None: it is whole.
pub fn tar_fault(tar: &Path) -> Option<String> {
    let f = match fs::File::open(tar) {
        Ok(f) => f,
        Err(e) => return Some(format!("it does not open: {}", e.kind())),
    };
    let mut a = tar::Archive::new(f);
    let entries = match a.entries() {
        Ok(e) => e,
        Err(e) => return Some(format!("it does not unpack: {}", plain(&e.to_string()))),
    };
    let (mut blobs, mut small) = (std::collections::BTreeSet::new(), std::collections::BTreeMap::new());
    let mut index = None;
    for entry in entries {
        let mut e = match entry {
            Ok(e) => e,
            Err(e) => return Some(format!("it does not unpack: {}", plain(&e.to_string()))),
        };
        let path = e.path().map(|p| p.to_string_lossy().trim_start_matches("./").to_string()).unwrap_or_default();
        // A name of the tar goes into a line of a terminal.
        let shown = plain(&path);
        let size = e.header().size().unwrap_or(0);
        let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
        let (mut buf, mut read, mut text) = (vec![0u8; 1 << 16], 0u64, Vec::new());
        loop {
            match e.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    ctx.update(&buf[..n]);
                    read += n as u64;
                    if size <= 1 << 16 {
                        text.extend_from_slice(&buf[..n]);
                    }
                }
                Err(err) => return Some(format!("{shown} does not unpack: {}", plain(&err.to_string()))),
            }
        }
        if read != size {
            return Some(format!("{shown} is cut: {read} of {size} bytes"));
        }
        if path == "index.json" {
            index = Some(text);
        } else if let Some(hex) = path.strip_prefix("blobs/sha256/") {
            let have: String = ctx.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect();
            if have != hex {
                return Some(format!("{shown}: its SHA-256 is {have}"));
            }
            blobs.insert(format!("sha256:{hex}"));
            small.insert(format!("sha256:{hex}"), text);
        }
    }
    let Some(index) = index else { return Some("no index.json".into()) };
    let digest_of = |v: &serde_json::Value| v["digest"].as_str().map(str::to_string);
    let index: serde_json::Value = serde_json::from_slice(&index).unwrap_or_default();
    let manifest = index["manifests"].get(0).and_then(digest_of);
    let Some(manifest) = manifest else { return Some("index.json names no manifest".into()) };
    if !blobs.contains(&manifest) {
        return Some(format!("no blob of the manifest {}", plain(&manifest)));
    }
    let m: serde_json::Value = small.get(&manifest).and_then(|t| serde_json::from_slice(t).ok()).unwrap_or_default();
    let layers = m["layers"].as_array().into_iter().flatten().filter_map(digest_of);
    m.get("config")
        .and_then(digest_of)
        .into_iter()
        .chain(layers)
        .find(|d| !blobs.contains(d))
        .map(|d| format!("no blob {}", plain(&d)))
}

/// `text` with each character that is not printable ASCII as `?`, cut to
/// 120 characters: the bytes of a broken tar are not text.
fn plain(text: &str) -> String {
    text.chars().take(120).map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '?' }).collect()
}

/// Remove each `*.tar` of `dir` but the `keep` newest, and each temporary
/// tar of a save that stopped.
fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut tars: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(t, p)| {
            let part = p.extension().is_some_and(|x| x.to_string_lossy().starts_with("part-"));
            if part && t.elapsed().is_ok_and(|age| age > STALE_PART) {
                let _ = std::fs::remove_file(p);
            }
            p.extension().is_some_and(|x| x == "tar")
        })
        .collect();
    tars.sort_by_key(|a| std::cmp::Reverse(a.0));
    for (_, p) in tars.into_iter().skip(keep) {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    /// T-630: a whole tar has no fault; a cut one, a changed blob and a
    /// missing layer each have one; a temporary tar of a save that
    /// stopped goes at the prune.
    #[test]
    fn the_fault_of_a_tar() {
        let d = crate::scratch::Scratch::new("t630-fault");
        let tar = d.join("i.tar");
        super::super::cache_tests::write_image(&tar);
        assert_eq!(super::tar_fault(&tar), None);
        let good = std::fs::read(&tar).unwrap();
        for (what, len) in [("cut in a blob", 20 * 1024), ("cut before the index", good.len() - 1536)] {
            std::fs::write(&tar, &good[..len]).unwrap();
            assert!(super::tar_fault(&tar).is_some(), "{what}");
        }
        let mut changed = good.clone();
        changed[1536 + 100] ^= 1;
        std::fs::write(&tar, &changed).unwrap();
        assert!(super::tar_fault(&tar).unwrap().contains("its SHA-256 is"));
        assert!(super::tar_fault(&d.join("none.tar")).unwrap().contains("does not open"));
        // The bytes of a broken tar never reach a terminal as they are.
        let noise: Vec<u8> = (0..5000u32).map(|i| (i * 7 + 27) as u8).collect();
        std::fs::write(&tar, &noise).unwrap();
        let fault = super::tar_fault(&tar).unwrap();
        assert!(fault.chars().all(|c| c.is_ascii_graphic() || c == ' ') && fault.len() < 200, "{fault}");
        let part = d.join("x.part-1");
        std::fs::write(&part, "p").unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
        std::fs::File::options().append(true).open(&part).unwrap().set_modified(old).unwrap();
        std::fs::write(d.join("y.part-2"), "p").unwrap();
        super::prune(&d, 4);
        assert!(!part.exists() && d.join("y.part-2").exists());
    }

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
