//! The images of a run by tag (T-130): `pod-actions-store` of podcrates
//! (GitHub #94) pins each image of `container:`, `services:` and
//! `docker://` steps that a tag names by the lock; the job message gives the
//! runner `NAME@sha256:HEX`, never the tag. The source of the images is
//! podvm: it pulls each tag into the store of the units.

use std::path::PathBuf;
use std::process::Command;

pub use pod_actions_store::images::*;

/// The images of podvm: `podvm pull` into the store of the units, then the
/// digest that the store gives for the tag.
pub struct PodvmPull {
    pub binary: PathBuf,
    pub store: PathBuf,
    pub platform: String,
}

impl ImageSource for PodvmPull {
    fn pull(&self, image: &str) -> Result<String, String> {
        let run = |args: &[&str]| -> Result<String, String> {
            let out = Command::new(&self.binary)
                .args(args)
                .env("PODVM_STORE", &self.store)
                .output()
                .map_err(|e| format!("{}: {e}", self.binary.display()))?;
            if !out.status.success() {
                return Err(format!("podvm {}: {}", args[0], String::from_utf8_lossy(&out.stderr).trim()));
            }
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        };
        run(&["pull", "-q", "--platform", &self.platform, image])?;
        let list =
            run(&["images", "--digests", "--no-trunc", "--format", "{{.Repository}}:{{.Tag}} {{.Digest}}", image])?;
        list.lines()
            .filter_map(|l| l.split_once(' '))
            .find_map(|(_, d)| d.trim().starts_with("sha256:").then(|| d.trim().to_string()))
            .ok_or_else(|| format!("podvm images: no digest for `{image}`: {list}"))
    }
}
