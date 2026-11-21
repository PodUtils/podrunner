//! The root of a running podvm unit (T-067). podvm copies the image into a
//! tree for each container at `create` (PodUtils/podvm@T-1709) and names it
//! in `podvm inspect --format '{{.RootfsPath}}'`; the runner writes in that
//! tree. The driver takes the tree that `inspect` names, so `observe` and
//! the volume sources of the hooks follow the unit (T-545: the tree must be
//! under the store, `store::own_tree`).

use std::path::{Path, PathBuf};
use std::process::Stdio;

use podrunner_schema::UnitId;

use super::PodvmUnit;

impl PodvmUnit {
    /// True when podvm says that the launcher of the container `name` was
    /// killed (`State` `dead`): its orphans went to init, out of reach of
    /// podvm and of the driver (design test 15).
    pub(super) fn launcher_died(&self, store: &Path, name: &str) -> bool {
        let Ok(out) = self
            .podvm
            .command()
            .args(["inspect", name])
            .env("PODVM_STORE", store)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            return false;
        };
        super::containers::inspected(&out.stdout).is_some_and(|c| super::containers::is_dead(&c))
    }

    /// `RootfsPath` of the container `name` in `store`, when podvm names
    /// a directory.
    pub(super) fn inspected_root(&self, store: &Path, name: &str) -> Option<PathBuf> {
        let out = self
            .podvm
            .command()
            .args(["inspect", "--format", "{{.RootfsPath}}", name])
            .env("PODVM_STORE", store)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let path = PathBuf::from(text.lines().rev().find(|l| !l.trim().is_empty())?.trim());
        (out.status.success() && path.is_dir()).then_some(path)
    }

    /// `OomKills` of `podvm inspect` of `unit`, where podvm gives it.
    pub(super) fn oom_kills_of(&self, unit: &UnitId) -> Option<u64> {
        let out = self
            .podvm
            .command()
            .args(["inspect", unit.as_str()])
            .env("PODVM_STORE", self.store())
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        let c = super::containers::inspected(&out.stdout)?;
        let v = &c["OomKills"];
        v.as_u64().or_else(|| v.as_str()?.parse().ok())
    }
}
