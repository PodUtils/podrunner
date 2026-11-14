//! The images of a run by tag (T-130). Before the first job, each image of
//! `container:`, `services:` and `docker://` steps that a tag names is
//! pulled into the store of the units and pinned by the lock file: the job
//! message gives the runner `NAME@sha256:HEX`, never the tag, and a unit
//! never pulls. A tag that moved since the lock fails by name.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use pod_actions_workflow::{Kind, Node, Workflow};

use crate::lock::Lock;
use crate::store::Mode;

/// A source of images: it pulls `NAME:TAG` into the store and gives its
/// digest `sha256:HEX`.
pub trait ImageSource {
    fn pull(&self, image: &str) -> Result<String, String>;
}

/// The image of a `container:` or of a service: the string, or `image:` of
/// the mapping.
fn image_of(node: &Node) -> Option<&str> {
    match &node.kind {
        Kind::String(s) => Some(s),
        Kind::Map(e) => e.iter().find(|(k, _)| k.name == "image").and_then(|(_, v)| match &v.kind {
            Kind::String(s) => Some(s.as_str()),
            _ => None,
        }),
        _ => None,
    }
}

/// An image that a tag names: no digest. `NAME` alone is `NAME:latest`, as
/// docker reads it.
pub fn by_tag(image: &str) -> Option<String> {
    if image.contains('@') || image.trim().is_empty() {
        return None;
    }
    let last = image.rsplit('/').next().unwrap_or(image);
    Some(if last.contains(':') { image.to_string() } else { format!("{image}:latest") })
}

/// Each image of the workflow that a tag names. An expression is refused:
/// the host pulls each image before the first job.
pub fn workflow_images(w: &Workflow) -> Result<Vec<String>, String> {
    let mut out = BTreeSet::new();
    let mut add = |image: &str, line: u32| -> Result<(), String> {
        if image.contains("${{") {
            return Err(format!("line {line}: the image {image:?} is an expression; name it, or pin it by digest"));
        }
        if let Some(t) = by_tag(image) {
            out.insert(t);
        }
        Ok(())
    };
    for job in &w.jobs {
        if let Some(c) = &job.container
            && let Some(i) = image_of(c)
        {
            add(i, c.mark.line)?;
        }
        if let Some(Node { kind: Kind::Map(services), .. }) = &job.services {
            for (_, s) in services {
                if let Some(i) = image_of(s) {
                    add(i, s.mark.line)?;
                }
            }
        }
        for step in &job.steps {
            if let Some(i) = step.uses.as_deref().and_then(|u| u.strip_prefix("docker://")) {
                add(i, step.mark.line)?;
            }
        }
    }
    Ok(out.into_iter().collect())
}

/// Pin each image of `images` by the lock and the source; the lock changes
/// for a new image. The result maps `docker://NAME:TAG` to `NAME@sha256:HEX`.
pub fn resolve(
    images: &[String],
    lock: &mut Lock,
    source: &dyn ImageSource,
    mode: Mode,
    lock_name: &str,
) -> Result<(Vec<(String, String)>, bool), String> {
    let mut out = Vec::new();
    let mut changed = false;
    for image in images {
        let known = lock.images.get(image).cloned();
        let digest = match (known, mode) {
            (Some(d), Mode::Offline) => d,
            (Some(d), _) => {
                let now = source.pull(image)?;
                if now != d {
                    return Err(format!(
                        "the image `{image}` moved: {lock_name} has {d}, the registry has {now}; remove its line from {lock_name} to take {now}"
                    ));
                }
                d
            }
            (None, Mode::Offline) => {
                return Err(format!("the image `{image}` is not in {lock_name}; run once with no --offline"));
            }
            (None, Mode::Frozen) => {
                return Err(format!("the image `{image}` is not in {lock_name}, and --frozen refuses a change"));
            }
            (None, Mode::Online) => {
                let d = source.pull(image)?;
                lock.images.insert(image.clone(), d.clone());
                changed = true;
                d
            }
        };
        let name =
            image.rsplit_once(':').filter(|(n, _)| !n.ends_with('/') && !n.is_empty()).map_or(image.as_str(), |x| x.0);
        out.push((format!("docker://{image}"), format!("{name}@{digest}")));
    }
    Ok((out, changed))
}

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

#[cfg(test)]
mod tests;
