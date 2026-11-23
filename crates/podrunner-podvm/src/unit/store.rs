//! The one store of the units of a driver (T-545). `STATE/store` holds
//! each pinned image once: the first unit of a digest loads its tar and
//! podvm extracts it; each later unit of that digest loads and extracts
//! nothing. Each unit is a container of its own in that store: `podvm run
//! -d` copies the extracted image into a tree of the container
//! (`<store>/trees/<id>/`, a `FICLONE` where the file system has it), and
//! podvm never writes the extracted image after the extraction, so each
//! unit starts from the image as the pin gives it. The credential files of
//! a unit stay in its own directory on the host and reach the unit as a
//! volume at `CREDENTIAL_DIR`: nothing of one unit is in the source of the
//! next. A rung that binds no volume keeps a store for each unit, with the
//! credential files in its own extracted image (T-023).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use podrunner_schema::{FileBytes, PlainName};
use podrunner_unit::{StartSpec, UnitCode, UnitError};

use super::{PodvmUnit, RUNNER_ROOT, WORKDIR, host, write_private};

/// The directory of the store of the units, under the state directory (and
/// of the store of a unit, under its directory).
pub(super) const STORE: &str = "store";

/// The directory of the credential files of a unit, under its directory.
const CREDENTIALS: &str = "runner-root";

impl PodvmUnit {
    /// The store of the units of this driver.
    pub(super) fn store(&self) -> PathBuf {
        self.state_dir.join(STORE)
    }

    /// The reference of the image of `tar` in `store`, by the digest of its
    /// manifest. The tar is loaded only when the store does not hold that
    /// digest. A store that does not hold the digest after the load fails
    /// the start: no unit runs an image that is not the pinned one.
    pub(super) fn loaded(&self, store: &Path, tar: &Path) -> Result<String, UnitError> {
        let digest = manifest_of(tar)
            .ok_or_else(|| UnitError::new(UnitCode::UnitStartFailed, format!("{} names no manifest", tar.display())))?;
        fs::create_dir_all(store).map_err(|e| host("the store of the units", e))?;
        if let Some(r) = self.holds(store, &digest)? {
            return Ok(r);
        }
        self.podvm(store, &["load", "-i", &tar.to_string_lossy()], None)?;
        self.holds(store, &digest)?.ok_or_else(|| {
            UnitError::new(UnitCode::UnitStartFailed, format!("podvm load gave no image of the digest {digest}"))
        })
    }

    /// `NAME@DIGEST` when `store` holds the manifest `digest`.
    fn holds(&self, store: &Path, digest: &str) -> Result<Option<String>, UnitError> {
        let list = self.podvm(store, &["images", "--format", "{{.Repository}}@{{.Digest}}"], None)?;
        Ok(reference_in(&list, digest))
    }

    /// The extracted image of `reference` in `store`: the source of each
    /// tree. The driver reads it and never writes it.
    pub(super) fn extracted(&self, store: &Path, reference: &str) -> Result<PathBuf, UnitError> {
        let out = self.podvm(store, &["extract", reference], None)?;
        let root = PathBuf::from(out.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim());
        if !root.is_dir() {
            return Err(UnitError::new(UnitCode::UnitStartFailed, "podvm extract names no rootfs"));
        }
        Ok(root)
    }

    /// The store of a unit in the directory `dir`: the store of the units
    /// where the rung binds a volume, else a store of its own.
    pub(super) fn store_of(&self, dir: &Path) -> PathBuf {
        if self.volumes { self.store() } else { dir.join(STORE) }
    }

    /// Where the driver writes the credential files of a unit: a directory
    /// of the unit that is its volume, or the runner root in the extracted
    /// image of a store of its own.
    pub(super) fn credential_root(&self, dir: &Path, image_root: &Path) -> PathBuf {
        if self.volumes {
            dir.join(CREDENTIALS)
        } else {
            image_root.join(WORKDIR.trim_start_matches('/')).join(RUNNER_ROOT)
        }
    }

    /// The credential files of `spec` in `root`, at mode 0600 in a
    /// directory of mode 0700.
    pub(super) fn place_credentials(&self, root: &Path, spec: &StartSpec) -> Result<(), UnitError> {
        fs::create_dir_all(root).map_err(|e| host("the runner root", e))?;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700)).map_err(|e| host("the runner root", e))?;
        for f in &spec.credential.files {
            if !f.name_is_plain() {
                return Err(UnitError::new(UnitCode::UnitStartFailed, "a credential file name that is not plain"));
            }
            let bytes = f.bytes().map_err(|e| UnitError::new(UnitCode::UnitStartFailed, e))?;
            write_private(&root.join(&f.name), &bytes)?;
        }
        Ok(())
    }

    /// The tree of a running unit and the host directory of its credential
    /// files `names`. In the store of the units the tree must be its own;
    /// in a store of its own it is the tree that `inspect` names, else the
    /// extracted image. Each file must be there at mode 0600: no unit runs
    /// with a credential that it cannot read, or that another user can.
    pub(super) fn root_of_unit(
        &self,
        store: &Path,
        image_root: &Path,
        inspected: Option<PathBuf>,
        creds: &Path,
        names: &[&str],
    ) -> Result<(PathBuf, PathBuf), UnitError> {
        let (tree, runner_root) = if self.volumes {
            (own_tree(store, inspected)?, creds.to_path_buf())
        } else {
            let tree = inspected.unwrap_or_else(|| image_root.to_path_buf());
            let runner_root = tree.join(WORKDIR.trim_start_matches('/')).join(RUNNER_ROOT);
            (tree, runner_root)
        };
        for name in names {
            let mode = fs::metadata(runner_root.join(name)).map(|m| m.permissions().mode() & 0o777);
            if mode.ok() != Some(0o600) {
                return Err(UnitError::new(
                    UnitCode::UnitStartFailed,
                    format!("the credential file {name} is not in the runner root of the unit at mode 0600"),
                ));
            }
        }
        Ok((tree, runner_root))
    }
}

/// The tree of a container: the directory that `inspect` names, under
/// `<store>/trees/`. Any other directory (none, or the extracted image
/// that a guest tier uses) is refused: a credential file written there
/// is in the source of each later unit.
pub(super) fn own_tree(store: &Path, inspected: Option<PathBuf>) -> Result<PathBuf, UnitError> {
    let trees = store.join("trees");
    match inspected {
        Some(t) if t.starts_with(&trees) && t != trees => Ok(t),
        Some(t) => Err(UnitError::new(
            UnitCode::UnitStartFailed,
            format!("the root of the unit {} is not a tree of its own under {}", t.display(), trees.display()),
        )),
        None => Err(UnitError::new(UnitCode::UnitStartFailed, "podvm inspect names no tree of the unit")),
    }
}

/// The digest of the manifest in `index.json` of the OCI tar `tar`. An
/// image pinned by the digest of an index loads as its platform manifest
/// (T-130), so the store holds the image under this digest.
pub(super) fn manifest_of(tar: &Path) -> Option<String> {
    let f = fs::File::open(tar).ok()?;
    let mut a = tar::Archive::new(f);
    let mut entry = a.entries().ok()?.flatten().find(|e| e.path().is_ok_and(|p| p.as_os_str() == "index.json"))?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut entry, &mut text).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v["manifests"][0]["digest"].as_str().map(str::to_string)
}

/// The line `NAME@DIGEST` of `podvm images --format` for `digest`.
fn reference_in(list: &str, digest: &str) -> Option<String> {
    list.lines()
        .map(str::trim)
        .find(|l| l.rsplit_once('@').is_some_and(|(name, d)| !name.is_empty() && d == digest))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_is_the_line_of_the_digest() {
        let a = format!("sha256:{}", "a".repeat(64));
        let b = format!("sha256:{}", "b".repeat(64));
        let list = format!("busybox@{a}\nrunner@{b}\n");
        assert_eq!(reference_in(&list, &b), Some(format!("runner@{b}")));
        assert_eq!(reference_in(&list, "sha256:c"), None);
        // A line with no name is not a reference.
        assert_eq!(reference_in(&format!("@{a}\n"), &a), None);
    }

    #[test]
    fn only_a_tree_under_the_store_is_the_root_of_a_unit() {
        let store = Path::new("/s");
        let tree = PathBuf::from("/s/trees/1a2b/rootfs");
        assert_eq!(own_tree(store, Some(tree.clone())).unwrap(), tree);
        // The extracted image (a guest tier runs in it), no tree, and the
        // directory of the trees itself are refused.
        for t in [Some(PathBuf::from("/s/rootfs/sha256_x/rootfs")), None, Some(PathBuf::from("/s/trees"))] {
            assert_eq!(own_tree(store, t).unwrap_err().code, UnitCode::UnitStartFailed);
        }
    }

    #[test]
    fn an_image_of_an_index_is_held_by_the_digest_of_its_loaded_manifest() {
        let dir = std::env::temp_dir().join(format!("podrunner-loaded-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tar_path = dir.join("i.tar");
        super::super::store_tests::write_tar(&tar_path, &format!("sha256:{}", "b".repeat(64)));
        assert_eq!(manifest_of(&tar_path), Some(format!("sha256:{}", "b".repeat(64))));
        assert_eq!(manifest_of(&dir.join("none.tar")), None, "no tar: no manifest");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
