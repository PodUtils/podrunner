//! The actions of a job from a local directory (T-091). A run service job
//! of runner v2.338.2 resolves each `uses: NAME@REF` with its launch client:
//! the job message names the endpoint (`system.github.launch_endpoint`), and
//! the runner POSTs the list of references to
//! `/actions/build/PLAN/jobs/JOB/runnerresolve/actions`. This broker answers
//! each reference with its own ref as the commit (the local mode takes only
//! actions pinned by a full commit id), and serves the tarball of each one
//! from `DIR/OWNER_REPO/SHA.tar.gz`: an archive with one top directory, as
//! GitHub's tarballs.

use std::path::Path;

use serde_json::{Map, Value, json};

/// The path of a tarball on this broker.
const TARBALLS: &str = "_actions/";

/// The name of an action in the archive directory: `/` becomes `_`, as in
/// the archive cache of the runner.
pub fn archive_name(name_with_owner: &str, sha: &str) -> String {
    format!("{}/{sha}.tar.gz", name_with_owner.replace('/', "_"))
}

/// A made-up token of the downloads: this broker checks no credential.
const TOKEN: &str = "podrunner-local-actions";

fn resolve(base: &str, body: &Value) -> Value {
    let mut out = Map::new();
    for a in body["actions"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let (Some(name), Some(version)) = (a["action"].as_str(), a["version"].as_str()) else { continue };
        out.insert(
            format!("{name}@{version}"),
            json!({
                "name": name, "resolved_name": name, "resolved_sha": version, "version": version,
                "tar_url": format!("{base}{TARBALLS}{}", archive_name(name, version)),
                "zip_url": format!("{base}{TARBALLS}{name}/{version}.zip"),
                "authentication": { "token": TOKEN, "expires_at": "2099-01-01T00:00:00Z" },
            }),
        );
    }
    json!({ "actions": out })
}

/// The path of the tarball of the local tree (T-092), on this broker: the
/// stand-in of `actions/checkout` reads it.
pub const CHECKOUT: &str = "_checkout.tar.gz";

/// The answer to a request of the actions, or `None` for another route.
/// `tree` is the tarball of the local tree.
pub fn route(
    method: &str,
    path: &str,
    body: &Value,
    base: &str,
    dir: Option<&Path>,
    tree: Option<&Path>,
) -> Option<(u16, Vec<u8>)> {
    let json = |v: Value| Some((200, v.to_string().into_bytes()));
    match (method, path) {
        ("GET", p) if p.strip_prefix('/') == Some(CHECKOUT) => match tree.map(std::fs::read) {
            Some(Ok(bytes)) => Some((200, bytes)),
            _ => Some((404, Vec::new())),
        },
        ("POST", p) if p.starts_with("/actions/build/") && p.ends_with("/runnerresolve/actions") => {
            json(resolve(base, body))
        }
        ("GET", p) if p.starts_with(&format!("/{TARBALLS}")) => {
            let rel = &p[TARBALLS.len() + 1..];
            // One name and one file: no other path of the directory.
            let ok = rel.split('/').count() == 2 && !rel.split('/').any(|c| c.is_empty() || c == "." || c == "..");
            match dir.filter(|_| ok).map(|d| std::fs::read(d.join(rel))) {
                Some(Ok(bytes)) => Some((200, bytes)),
                _ => Some((404, Vec::new())),
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_routes_of_the_actions() {
        let dir = std::env::temp_dir().join(format!("podrunner-actions-{}", std::process::id()));
        let sha = "a".repeat(40);
        std::fs::create_dir_all(dir.join("actions_checkout")).unwrap();
        std::fs::write(dir.join(archive_name("actions/checkout", &sha)), b"tgz").unwrap();
        let body = json!({ "actions": [{ "action": "actions/checkout", "version": sha }] });
        let path = "/actions/build/p/jobs/j/runnerresolve/actions";
        let (status, out) = route("POST", path, &body, "http://h/", None, None).unwrap();
        assert_eq!(status, 200);
        let v: Value = serde_json::from_slice(&out).unwrap();
        let info = &v["actions"][format!("actions/checkout@{sha}")];
        assert_eq!(info["resolved_sha"], sha.as_str());
        let url = info["tar_url"].as_str().unwrap();
        assert_eq!(url, format!("http://h/_actions/actions_checkout/{sha}.tar.gz"));
        let path = url.trim_start_matches("http://h");
        assert_eq!(route("GET", path, &Value::Null, "", Some(&dir), None).unwrap(), (200, b"tgz".to_vec()));
        // No other file of the directory, and no file with no directory.
        for bad in ["/_actions/../x.tar.gz", "/_actions/a/../../x", "/_actions/x.tar.gz", "/_actions/a/b/c"] {
            assert_eq!(route("GET", bad, &Value::Null, "", Some(&dir), None).unwrap().0, 404, "{bad}");
        }
        assert_eq!(route("GET", path, &Value::Null, "", None, None).unwrap().0, 404, "no directory");
        assert!(route("GET", "/message", &Value::Null, "", None, None).is_none());
        // The tree of the checkout, when the run gives one.
        let tree = dir.join("tree.tar.gz");
        std::fs::write(&tree, b"tree").unwrap();
        assert_eq!(
            route("GET", "/_checkout.tar.gz", &Value::Null, "", None, Some(&tree)).unwrap(),
            (200, b"tree".to_vec())
        );
        assert_eq!(route("GET", "/_checkout.tar.gz", &Value::Null, "", None, None).unwrap().0, 404);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
