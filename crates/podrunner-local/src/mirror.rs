//! The mirror of a guest with an engine (T-157): each pinned image of
//! Docker Hub goes from the store of the units (`podvm save`) into one OCI
//! layout, which the broker serves as the registry mirror of the engine
//! (`podrunner_broker_fake::mirror`). The store keeps the manifest of the
//! platform of the unit, not the index: the job message of a guest names
//! that manifest, so the engine asks the mirror for a digest that it has.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;
use std::process::Command;

pub use podrunner_broker_fake::mirror::Mirror;

use pod_actions_workflow::{Kind, Node, Workflow};

use crate::images::PodvmPull;

/// Docker Hub holds `NAME`: its first part names no registry host.
pub fn on_docker_hub(name: &str) -> bool {
    match name.split_once('/') {
        None => true,
        Some((first, _)) => {
            first == "docker.io" || first == "index.docker.io" || !(first.contains(['.', ':']) || first == "localhost")
        }
    }
}

/// Unpack the OCI layout of `tar` into `dir`; give the digest of its
/// first manifest (`index.json`).
fn unpack(tar: &Path, dir: &Path) -> Result<String, String> {
    let file = std::fs::File::open(tar).map_err(|e| format!("{}: {e}", tar.display()))?;
    let mut archive = tar::Archive::new(file);
    let blobs = dir.join("blobs/sha256");
    std::fs::create_dir_all(&blobs).map_err(|e| format!("{}: {e}", blobs.display()))?;
    let mut manifest = None;
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        let parts: Vec<_> = path.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        match parts.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
            ["index.json"] => {
                let mut text = String::new();
                entry.read_to_string(&mut text).map_err(|e| e.to_string())?;
                let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("index.json: {e}"))?;
                manifest = v["manifests"][0]["digest"].as_str().map(str::to_string);
            }
            ["blobs", "sha256", hex] if hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()) => {
                let target = blobs.join(hex);
                if !target.exists() {
                    entry.unpack(&target).map_err(|e| format!("{}: {e}", target.display()))?;
                }
            }
            _ => {}
        }
    }
    manifest.ok_or_else(|| format!("{}: no manifest in index.json", tar.display()))
}

/// The image of a `container:` or of a service: the string, or `image:`
/// of the mapping.
fn image_of(node: &Node) -> Option<&str> {
    match &node.kind {
        Kind::String(s) => Some(s),
        Kind::Map(_) => match &node.get("image")?.kind {
            Kind::String(s) => Some(s.as_str()),
            _ => None,
        },
        _ => None,
    }
}

/// Each image of the workflow that a digest names (`NAME@sha256:HEX`),
/// sorted, each once. An image that a tag names is in the pins.
pub fn by_digest(w: &Workflow) -> Vec<String> {
    let mut out = BTreeSet::new();
    let mut add = |i: &str| {
        if i.contains('@') && !i.contains("${{") {
            out.insert(i.to_string());
        }
    };
    for job in &w.jobs {
        if let Some(i) = job.container.as_ref().and_then(image_of) {
            add(i);
        }
        if let Some(Node { kind: Kind::Map(services), .. }) = &job.services {
            for (_, s) in services {
                if let Some(i) = image_of(s) {
                    add(i);
                }
            }
        }
        for step in &job.steps {
            if let Some(i) = step.uses.as_deref().and_then(|u| u.strip_prefix("docker://")) {
                add(i);
            }
        }
    }
    out.into_iter().collect()
}

fn podvm(pull: &PodvmPull, args: &[&str]) -> Result<(), String> {
    let out = Command::new(&pull.binary)
        .args(args)
        .env("PODVM_STORE", &pull.store)
        .output()
        .map_err(|e| format!("{}: {e}", pull.binary.display()))?;
    match out.status.success() {
        true => Ok(()),
        false => Err(format!("podvm {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim())),
    }
}

/// Fill `dir` with each image of Docker Hub of the run: each pinned image
/// of `pins` (`docker://NAME:TAG` → `NAME@sha256:HEX`), and each image of
/// `digests`. Each image goes through a private store of one platform:
/// `podvm save` has no `--platform`, and the store of the units can hold
/// an image of each platform for one index digest. Give the pins for the
/// job messages of a guest: each image of the mirror by its manifest
/// (`docker://NAME@sha256:HEX` for an image that a digest names).
pub fn fill(
    pull: &PodvmPull,
    pins: &BTreeMap<String, String>,
    digests: &[String],
    dir: &Path,
) -> Result<BTreeMap<String, String>, String> {
    let mut out = pins.clone();
    let tagged = pins.iter().filter(|(k, _)| k.starts_with("docker://")).map(|(k, v)| (k.clone(), v.clone()));
    let named = digests.iter().map(|d| (format!("docker://{d}"), d.clone()));
    let one = PodvmPull { binary: pull.binary.clone(), store: dir.join("store"), platform: pull.platform.clone() };
    for (key, pinned) in tagged.chain(named).collect::<Vec<_>>() {
        let Some((name, _)) = pinned.split_once('@') else { continue };
        if !on_docker_hub(name) {
            continue;
        }
        let _ = std::fs::remove_dir_all(&one.store);
        std::fs::create_dir_all(&one.store).map_err(|e| format!("{}: {e}", one.store.display()))?;
        let tar = dir.join("save.tar");
        let saved = podvm(&one, &["pull", "-q", "--platform", &one.platform, &pinned])
            .and_then(|()| podvm(&one, &["save", "-o", &tar.to_string_lossy(), &pinned]));
        let manifest = saved.and_then(|()| unpack(&tar, dir));
        let _ = std::fs::remove_file(&tar);
        let _ = std::fs::remove_dir_all(&one.store);
        out.insert(key, format!("{name}@{}", manifest?));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_of_docker_hub() {
        for n in ["busybox", "library/redis", "bitnami/redis", "docker.io/library/redis"] {
            assert!(on_docker_hub(n), "{n}");
        }
        for n in ["ghcr.io/o/x", "localhost/x", "registry:5000/x", "quay.io/x/y"] {
            assert!(!on_docker_hub(n), "{n}");
        }
    }

    #[test]
    fn the_images_that_a_digest_names() {
        let d = format!("busybox@sha256:{}", "a".repeat(64));
        let text = format!(
            "on: push\njobs:\n  a:\n    runs-on: x\n    container: {d}\n    services:\n      web:\n        image: nginx:1\n    steps:\n      - uses: docker://{d}\n      - run: x\n"
        );
        let w = pod_actions_workflow::parse(&text).unwrap().workflow;
        assert_eq!(by_digest(&w), vec![d]);
    }

    #[test]
    fn a_saved_image_goes_into_the_layout() {
        let dir = std::env::temp_dir().join(format!("podrunner-mirror-fill-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (m, l) = ("a".repeat(64), "b".repeat(64));
        let index = format!(r#"{{"manifests":[{{"digest":"sha256:{m}"}}]}}"#);
        let mut b = tar::Builder::new(Vec::new());
        for (path, data) in [
            ("index.json".to_string(), index.as_bytes()),
            (format!("blobs/sha256/{m}"), b"{}".as_slice()),
            (format!("blobs/sha256/{l}"), b"layer".as_slice()),
            ("oci-layout".to_string(), b"{}".as_slice()),
        ] {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            b.append_data(&mut h, path, data).unwrap();
        }
        let tar = dir.join("x.tar");
        std::fs::write(&tar, b.into_inner().unwrap()).unwrap();
        let out = dir.join("layout");
        assert_eq!(unpack(&tar, &out).unwrap(), format!("sha256:{m}"));
        assert_eq!(std::fs::read(out.join("blobs/sha256").join(&l)).unwrap(), b"layer");
        assert!(!out.join("oci-layout").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
