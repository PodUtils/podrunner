//! The one store of the units of a driver (T-545). `STATE/store` holds
//! each pinned image once: the first unit of a digest loads its tar and
//! podvm extracts it; each later unit of that digest loads and extracts
//! nothing. Each unit is a container of its own in that store: `podvm
//! create` copies the extracted image into a tree of the container
//! (`<store>/trees/<id>/`, a `FICLONE` where the file system has it), the
//! driver writes the credential files into that tree, and `podvm start`
//! starts the payload. podvm never writes the extracted image after the
//! extraction, so each unit starts from the image as the pin gives it.
//! The credential files go only into the tree of the unit, never into the
//! extracted image that the next unit copies.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use podrunner_schema::PlainName;
use podrunner_unit::{StartSpec, UnitCode, UnitError};

use super::{PodvmUnit, RUNNER_ROOT, WORKDIR, host, write_private};

/// The directory of the store of the units, under the state directory.
pub(super) const STORE: &str = "store";

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

    /// `podvm create` of `argv` (it starts with `create`), the credential
    /// files of `spec` into the tree of the new container, then `podvm
    /// start` under the hold of `egress`. It gives the tree.
    pub(super) fn launch(
        &self,
        store: &Path,
        argv: &[&str],
        spec: &StartSpec,
        tag: &str,
        egress: Option<&Path>,
    ) -> Result<PathBuf, UnitError> {
        self.podvm(store, argv, None)?;
        let name = spec.unit.as_str();
        let tree = own_tree(store, self.inspected_root(store, name))?;
        let runner_root = tree.join(WORKDIR.trim_start_matches('/')).join(RUNNER_ROOT);
        fs::create_dir_all(&runner_root).map_err(|e| host("the runner root", e))?;
        fs::set_permissions(&runner_root, fs::Permissions::from_mode(0o700)).map_err(|e| host("the runner root", e))?;
        for f in &spec.credential.files {
            if !f.name_is_plain() {
                return Err(UnitError::new(UnitCode::UnitStartFailed, "a credential file name that is not plain"));
            }
            write_private(&runner_root.join(&f.name), f.content.expose())?;
        }
        self.podvm_held(store, &["start", name], Some(tag), egress)?;
        Ok(tree)
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
