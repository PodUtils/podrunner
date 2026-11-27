//! The Docker actions of a job (T-270): an action of a repository whose
//! `action.yml` has `runs.using: docker`, also in a composite action. The
//! runner builds or pulls its image with the docker of the host, so on a
//! runner directory such a job must fail before it starts.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use pod_actions_workflow::yaml::{Kind, Limits, load};

/// The text of `action.yml` (or `action.yaml`) at `path` of the tarball of
/// `repo` at `commit` in the store `dir`; the tar has one top directory.
pub(crate) fn action_text(dir: &Path, repo: &str, commit: &str, path: &str) -> Option<String> {
    let file = std::fs::File::open(dir.join(pod_actions_store::archive_name(repo, commit))).ok()?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let want: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
    for entry in tar.entries().ok()? {
        let mut entry = entry.ok()?;
        let name = entry.path().ok()?.to_string_lossy().into_owned();
        let parts: Vec<&str> = name.split('/').filter(|c| !c.is_empty()).collect();
        let Some((file, dirs)) = parts.split_last() else { continue };
        if dirs.len() == want.len() + 1 && dirs[1..] == want[..] && (*file == "action.yml" || *file == "action.yaml") {
            let mut text = String::new();
            entry.read_to_string(&mut text).ok()?;
            return Some(text);
        }
    }
    None
}

/// The text of `action.yml` (or `action.yaml`) at `path` (`./PATH`) of the
/// tarball of the tree of `--checkout`, which has no top directory (T-287).
fn tree_action_text(tree: &Path, path: &str) -> Option<String> {
    let file = std::fs::File::open(tree).ok()?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let want: Vec<&str> = path.split('/').filter(|c| !c.is_empty() && *c != ".").collect();
    for entry in tar.entries().ok()? {
        let mut entry = entry.ok()?;
        let name = entry.path().ok()?.to_string_lossy().into_owned();
        let parts: Vec<&str> = name.split('/').filter(|c| !c.is_empty() && *c != ".").collect();
        let Some((file, dirs)) = parts.split_last() else { continue };
        if dirs == want.as_slice() && (*file == "action.yml" || *file == "action.yaml") {
            let mut text = String::new();
            entry.read_to_string(&mut text).ok()?;
            return Some(text);
        }
    }
    None
}

/// Where the actions of a job are: the store and its pins, and the tarball
/// of the tree for a `./` action.
pub struct Sources<'a> {
    pub store: Option<&'a Path>,
    pub pins: &'a BTreeMap<String, String>,
    pub tree: Option<&'a Path>,
}

/// Each Docker action that `uses` (`OWNER/REPO[/PATH]@REF`, or `./PATH` of
/// the tree) is or holds, to the depth of composite actions that GitHub
/// allows (10).
/// With `build`, only an action whose image is a Dockerfile (a `docker://`
/// image needs no build).
fn docker_of_kind(uses: &str, src: &Sources, depth: u8, out: &mut Vec<String>, build: bool) {
    let text = match uses.strip_prefix("./") {
        // With no tarball of a tree, the tree is the current directory.
        Some(path) => match src.tree {
            Some(t) => tree_action_text(t, path),
            None => {
                ["action.yml", "action.yaml"].iter().find_map(|f| std::fs::read_to_string(Path::new(path).join(f)).ok())
            }
        },
        None => (|| {
            let (left, r) = uses.rsplit_once('@')?;
            let mut parts = left.splitn(3, '/');
            let (owner, repo) = (parts.next()?, parts.next()?);
            let repo = format!("{owner}/{repo}");
            let commit = src.pins.get(&format!("{repo}@{r}"))?;
            action_text(src.store?, &repo, commit, parts.next().unwrap_or(""))
        })(),
    };
    let Some(text) = text else { return };
    let Ok(root) = load(&text, Limits::default()) else { return };
    let Some(runs) = root.get("runs") else { return };
    match runs.get("using").map(|n| &n.kind) {
        Some(Kind::String(u)) if u.eq_ignore_ascii_case("docker") => {
            let image = runs.get("image").and_then(|n| match &n.kind {
                Kind::String(i) => Some(i.clone()),
                _ => None,
            });
            if !build || !image.is_some_and(|i| i.starts_with("docker://")) {
                out.push(uses.to_string());
            }
        }
        Some(Kind::String(u)) if u == "composite" && depth < 10 => {
            let Some(Kind::Seq(steps)) = runs.get("steps").map(|n| &n.kind) else { return };
            for s in steps {
                if let Some(Kind::String(u)) = s.get("uses").map(|n| &n.kind) {
                    match u.strip_prefix("docker://") {
                        Some(_) if !build => out.push(u.clone()),
                        Some(_) => {}
                        // `$/PATH`: the repository of this action (T-729).
                        None => match crate::store::self_of(u, uses) {
                            Some(at) => docker_of_kind(&at, src, depth + 1, out, build),
                            None => docker_of_kind(u, src, depth + 1, out, build),
                        },
                    }
                }
            }
        }
        _ => {}
    }
}

/// Each Docker action of the steps of the job `message`.
pub fn docker_actions(message: &crate::Job, src: &Sources) -> Vec<String> {
    actions_of(message, src, false)
}

/// The refusal of a job with an action that builds a Dockerfile, where the
/// unit has no engine: the hooks of podvm run images and build none
/// (T-237). An action of a `docker://` image runs through the hooks.
pub fn build_refusal(message: &crate::Job, src: &Sources) -> Option<String> {
    let found = actions_of(message, src, true);
    (!found.is_empty()).then(|| {
        format!(
            "the job uses the Docker action {}, which builds a Dockerfile: the hooks of a podvm unit build no image; run it with --microvm and --engine (a host with /dev/kvm)",
            found.join(", ")
        )
    })
}

fn actions_of(message: &crate::Job, src: &Sources, build: bool) -> Vec<String> {
    let mut out = Vec::new();
    let steps = message.overrides.get("steps").and_then(|s| s.as_array()).cloned().unwrap_or_default();
    for s in &steps {
        let r = &s["reference"];
        if r["type"] != "repository" {
            continue;
        }
        if r["repositoryType"] == "self" {
            if let Some(p) = r["path"].as_str() {
                let p = if p.starts_with("./") { p.to_string() } else { format!("./{p}") };
                docker_of_kind(&p, src, 0, &mut out, build);
            }
            continue;
        }
        let (Some(name), Some(git_ref)) = (r["name"].as_str(), r["ref"].as_str()) else { continue };
        let uses = match r["path"].as_str().filter(|p| !p.is_empty()) {
            Some(p) => format!("{name}/{p}@{git_ref}"),
            None => format!("{name}@{git_ref}"),
        };
        docker_of_kind(&uses, src, 0, &mut out, build);
    }
    out.dedup();
    out
}

/// The refusal of a job with Docker actions on a runner directory, if any.
pub fn refusal(message: &crate::Job, src: &Sources) -> Option<String> {
    let found = docker_actions(message, src);
    (!found.is_empty()).then(|| {
        format!(
            "the job uses the Docker action {}: run it in a unit (--podvm, or --microvm with --engine), not on a runner directory",
            found.join(", ")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tarball `OWNER_REPO/COMMIT.tar.gz` with the files `files` under one
    /// top directory.
    fn put(dir: &Path, repo: &str, commit: &str, files: &[(&str, &str)]) {
        let path = dir.join(pod_actions_store::archive_name(repo, commit));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let gz = flate2::write::GzEncoder::new(std::fs::File::create(&path).unwrap(), flate2::Compression::fast());
        let mut b = tar::Builder::new(gz);
        for (name, text) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(text.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, format!("top/{name}"), text.as_bytes()).unwrap();
        }
        b.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn a_docker_action_and_one_in_a_composite_are_found() {
        let dir = std::env::temp_dir().join(format!("podrunner-docker-actions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        put(&dir, "o/d", "c1", &[("action.yml", "runs:\n  using: docker\n  image: Dockerfile\n")]);
        put(&dir, "o/n", "c2", &[("action.yml", "runs:\n  using: node20\n  main: x.js\n")]);
        put(&dir, "o/c", "c3", &[("sub/action.yaml", "runs:\n  using: composite\n  steps:\n    - uses: o/d@v1\n")]);
        let pins: BTreeMap<String, String> =
            [("o/d@v1", "c1"), ("o/n@v2", "c2"), ("o/c@v3", "c3")].map(|(k, v)| (k.into(), v.into())).into();
        let step = |name: &str, r: &str, path: Option<&str>| {
            let mut v = serde_json::json!({ "reference": { "type": "repository", "repositoryType": "GitHub", "name": name, "ref": r } });
            if let Some(p) = path {
                v["reference"]["path"] = serde_json::json!(p);
            }
            v
        };
        let mut job = crate::Job::scripts("a", &["true"]);
        job.overrides.insert("steps".into(), serde_json::json!([step("o/n", "v2", None)]));
        assert!(
            refusal(&job, &Sources { store: Some(&dir), pins: &pins, tree: None }).is_none(),
            "a node action passes"
        );
        job.overrides.insert("steps".into(), serde_json::json!([step("o/c", "v3", Some("sub"))]));
        let e = refusal(&job, &Sources { store: Some(&dir), pins: &pins, tree: None })
            .expect("a composite with a Docker action");
        assert!(e.contains("o/d@v1") && e.contains("--podvm"), "{e}");
        job.overrides.insert("steps".into(), serde_json::json!([step("o/d", "v1", None)]));
        assert!(refusal(&job, &Sources { store: Some(&dir), pins: &pins, tree: None }).is_some());
        // T-237: a Dockerfile needs a build, a `docker://` image does not.
        let src = Sources { store: Some(&dir), pins: &pins, tree: None };
        assert!(build_refusal(&job, &src).is_some_and(|e| e.contains("builds a Dockerfile")));
        put(&dir, "o/i", "c4", &[("action.yml", "runs:\n  using: docker\n  image: docker://alpine@sha256:00\n")]);
        let pins2: BTreeMap<String, String> = [("o/i@v4".to_string(), "c4".to_string())].into();
        job.overrides.insert("steps".into(), serde_json::json!([step("o/i", "v4", None)]));
        let src2 = Sources { store: Some(&dir), pins: &pins2, tree: None };
        assert!(build_refusal(&job, &src2).is_none(), "an image action needs no build");
        assert!(refusal(&job, &src2).is_some(), "a runner directory runs neither");
        // T-729: `$/PATH` in a composite names the repository of that action.
        let docker = "runs:\n  using: docker\n  image: Dockerfile\n";
        put(
            &dir,
            "o/s",
            "c5",
            &[("action.yml", "runs:\n  using: composite\n  steps: [{uses: $/dock}]\n"), ("dock/action.yml", docker)],
        );
        let pins3: BTreeMap<String, String> = [("o/s@v5".to_string(), "c5".to_string())].into();
        job.overrides.insert("steps".into(), serde_json::json!([step("o/s", "v5", None)]));
        let e = refusal(&job, &Sources { store: Some(&dir), pins: &pins3, tree: None }).expect("a Docker action by $/");
        assert!(e.contains("o/s/dock@v5"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_docker_action_of_the_tree_is_found() {
        let dir = std::env::temp_dir().join(format!("podrunner-docker-tree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = dir.join("tree/.github/actions");
        std::fs::create_dir_all(a.join("dact")).unwrap();
        std::fs::create_dir_all(a.join("comp")).unwrap();
        std::fs::write(a.join("dact/action.yml"), "runs:\n  using: docker\n  image: Dockerfile\n").unwrap();
        std::fs::write(
            a.join("comp/action.yml"),
            "runs:\n  using: composite\n  steps:\n    - uses: docker://alpine@sha256:00\n",
        )
        .unwrap();
        let tree = crate::tree::tarball(&dir.join("tree"), &dir.join("scratch"), Default::default()).unwrap();
        let pins = BTreeMap::new();
        let src = Sources { store: None, pins: &pins, tree: Some(&tree) };
        for path in ["./.github/actions/dact", "./.github/actions/comp"] {
            let mut job = crate::Job::scripts("a", &["true"]);
            job.overrides.insert(
                "steps".into(),
                serde_json::json!([{ "reference": { "type": "repository", "repositoryType": "self", "path": path } }]),
            );
            assert!(refusal(&job, &src).is_some(), "{path}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
