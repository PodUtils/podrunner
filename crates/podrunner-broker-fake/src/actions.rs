//! The actions of a job from a local directory (T-091). A run service job
//! of runner v2.338.2 resolves each `uses: NAME@REF` with its launch client:
//! the job message names the endpoint (`system.github.launch_endpoint`), and
//! the runner POSTs the list of references to
//! `/actions/build/PLAN/jobs/JOB/runnerresolve/actions`. This broker answers
//! each reference from the pins of the run (`OWNER/REPO@REF` to a commit,
//! T-118), or with its own ref when that is a full commit id, and serves the
//! tarball of each one
//! from `DIR/OWNER_REPO/SHA.tar.gz`: an archive with one top directory, as
//! GitHub's tarballs.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Map, Value, json};

/// The path of a tarball on this broker.
const TARBALLS: &str = "_actions/";

/// The name of an action in the archive directory: `/` becomes `_`, as in
/// the archive cache of the runner.
pub fn archive_name(name_with_owner: &str, sha: &str) -> String {
    format!("{}/{sha}.tar.gz", name_with_owner.replace('/', "_"))
}

fn stand_in_file() -> String {
    archive_name(crate::stand_in::NAME, crate::stand_in::SHA)
}

/// A made-up token of the downloads: this broker checks no credential.
const TOKEN: &str = "podrunner-local-actions";

/// Each ref of it resolves to the stand-in, also in a step of an action
/// that the job message does not change (T-708): the unit reaches no forge.
const CHECKOUT_ACTION: &str = "actions/checkout";

/// The commit of `NAME@VERSION`: its pin, or the version when it is a full
/// commit id.
fn commit<'a>(pins: &'a BTreeMap<String, String>, name: &str, version: &'a str) -> Option<&'a str> {
    if name == crate::stand_in::NAME || name == CHECKOUT_ACTION {
        return Some(crate::stand_in::SHA);
    }
    let full = version.len() == 40 && version.bytes().all(|b| b.is_ascii_hexdigit());
    pins.get(&format!("{name}@{version}")).map(String::as_str).or(full.then_some(version))
}

/// The name of the tarball that answers `name`.
fn served(name: &str) -> &str {
    if name == CHECKOUT_ACTION { crate::stand_in::NAME } else { name }
}

fn resolve(base: &str, body: &Value, pins: &BTreeMap<String, String>) -> Value {
    let mut out = Map::new();
    for a in body["actions"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let (Some(name), Some(version)) = (a["action"].as_str(), a["version"].as_str()) else { continue };
        // A reference with no pin gets no answer: the runner names it.
        let Some(sha) = commit(pins, name, version) else { continue };
        out.insert(
            format!("{name}@{version}"),
            json!({
                "name": name, "resolved_name": name, "resolved_sha": sha, "version": version,
                "tar_url": format!("{base}{TARBALLS}{}", archive_name(served(name), sha)),
                "zip_url": format!("{base}{TARBALLS}{name}/{sha}.zip"),
                "authentication": { "token": TOKEN, "expires_at": "2099-01-01T00:00:00Z" },
            }),
        );
    }
    json!({ "actions": out })
}

/// The path of the tarball of the local tree (T-092), on this broker: the
/// stand-in of `actions/checkout` reads it.
pub const CHECKOUT: &str = "_checkout.tar.gz";

/// The path of the tarball of a commit of the local git on this broker
/// (T-640): the stand-in of a checkout with `ref:` reads it.
pub fn checkout_at(commit: &str) -> String {
    format!("_checkout/{commit}.tar.gz")
}

/// The file of that tarball, beside the tarball of the local tree.
pub fn checkout_file(commit: &str) -> String {
    format!("tree-{commit}.tar.gz")
}

/// The prefix of the key of the pins for `ref: R` of a checkout of the
/// same repository (T-640): `checkout-ref:R`.
pub const REF_PIN: &str = "checkout-ref:";

/// The path on this broker of the route of a pinned tree (T-713): the
/// stand-in of a checkout in an action asks it for `KEY` of the pins,
/// with percent escapes.
pub const PIN: &str = "_checkout/pin/";

/// The answer for the pinned tree of `key` (with percent escapes): the
/// route of its tarball on this broker (and `strip` for a tarball of the
/// store), or the reason that fails the step. A key with no pin is 404:
/// the run pins only the static values of a checkout in a local action.
fn pinned(key: &str, pins: &BTreeMap<String, String>) -> (u16, Vec<u8>) {
    let none = (404, Vec::new());
    let Some(value) = unescape(key).and_then(|k| pins.get(&k).map(|v| (k, v.as_str()))) else { return none };
    let answer = match value {
        (k, "") if k.starts_with(REF_PIN) => json!({ "route": CHECKOUT }),
        (k, why) if k.starts_with(REF_PIN) && why.starts_with('!') => json!({ "fail": &why[1..] }),
        (k, sha) if k.starts_with(REF_PIN) && commit_id(sha) => json!({ "route": checkout_at(sha) }),
        // `OWNER/REPO@REF` of another repository, from the store.
        (k, sha) if commit_id(sha) => {
            let repo = k.split_once('@').map_or("", |(r, _)| r);
            let name = |p: &str| {
                !p.is_empty() && p != "." && p != ".." && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            };
            if repo.split('/').count() != 2 || !repo.split('/').all(name) {
                return none;
            }
            json!({ "route": format!("{TARBALLS}{}", archive_name(repo, sha)), "strip": "1" })
        }
        _ => return none,
    };
    (200, answer.to_string().into_bytes())
}

/// `s` with each `%XX` decoded, or `None` for a bad escape or text that
/// is not UTF-8.
fn unescape(s: &str) -> Option<String> {
    let (b, mut out, mut i) = (s.as_bytes(), Vec::new(), 0);
    while i < b.len() {
        match b[i] {
            b'%' => {
                let hex = b.get(i + 1..i + 3).filter(|h| h.iter().all(u8::is_ascii_hexdigit))?;
                out.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
                i += 3;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// A full commit id of SHA-1 or SHA-256: no other name reaches the disk.
fn commit_id(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The answer to a request of the actions, or `None` for another route.
/// `tree` is the tarball of the local tree.
pub fn route(
    method: &str,
    path: &str,
    body: &Value,
    base: &str,
    dir: Option<&Path>,
    pins: &BTreeMap<String, String>,
    tree: Option<&Path>,
) -> Option<(u16, Vec<u8>)> {
    let json = |v: Value| Some((200, v.to_string().into_bytes()));
    match (method, path) {
        ("GET", p) if p.strip_prefix('/') == Some(CHECKOUT) => match tree.map(|t| (t, std::fs::read(t))) {
            Some((_, Ok(bytes))) => Some((200, bytes)),
            // A tree that went during the run (T-304): name it.
            Some((t, Err(e))) => {
                eprintln!("podrunner: the tree of --checkout ({}) cannot be read: {e}", t.display());
                Some((404, Vec::new()))
            }
            None => Some((404, Vec::new())),
        },
        ("GET", p) if p.strip_prefix('/').is_some_and(|p| p.starts_with(PIN)) => Some(pinned(&p[PIN.len() + 1..], pins)),
        ("GET", p) if p.starts_with("/_checkout/") => {
            let commit = p.strip_prefix("/_checkout/").and_then(|r| r.strip_suffix(".tar.gz")).filter(|c| commit_id(c));
            let file = commit.zip(tree.and_then(Path::parent)).map(|(c, d)| d.join(checkout_file(c)));
            match file.map(std::fs::read) {
                Some(Ok(bytes)) => Some((200, bytes)),
                _ => Some((404, Vec::new())),
            }
        }
        ("POST", p) if p.starts_with("/actions/build/") && p.ends_with("/runnerresolve/actions") => {
            json(resolve(base, body, pins))
        }
        // The stand-in of `actions/checkout` is in this binary (T-697).
        ("GET", p) if p.strip_prefix('/') == Some(&format!("{TARBALLS}{}", stand_in_file())) => {
            Some((200, crate::stand_in::tarball()))
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

    fn path_resolve() -> &'static str {
        "/actions/build/p/jobs/j/runnerresolve/actions"
    }

    #[test]
    fn the_routes_of_the_actions() {
        let dir = std::env::temp_dir().join(format!("podrunner-actions-{}", std::process::id()));
        let sha = "a".repeat(40);
        std::fs::create_dir_all(dir.join("actions_cache")).unwrap();
        std::fs::write(dir.join(archive_name("actions/cache", &sha)), b"tgz").unwrap();
        let body = json!({ "actions": [{ "action": "actions/cache", "version": sha }] });
        let path = "/actions/build/p/jobs/j/runnerresolve/actions";
        let (status, out) = route("POST", path, &body, "http://h/", None, &BTreeMap::new(), None).unwrap();
        assert_eq!(status, 200);
        let v: Value = serde_json::from_slice(&out).unwrap();
        let info = &v["actions"][format!("actions/cache@{sha}")];
        assert_eq!(info["resolved_sha"], sha.as_str());
        let url = info["tar_url"].as_str().unwrap();
        assert_eq!(url, format!("http://h/_actions/actions_cache/{sha}.tar.gz"));
        let path = url.trim_start_matches("http://h");
        // A tag answers from the pins; a tag with no pin gets no answer.
        let tag = json!({ "actions": [{ "action": "o/r", "version": "v1" }] });
        let pins = BTreeMap::from([("o/r@v1".to_string(), sha.clone())]);
        let (_, out) =
            route("POST", "/actions/build/p/jobs/j/runnerresolve/actions", &tag, "http://h/", None, &pins, None)
                .unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["actions"]["o/r@v1"]["resolved_sha"], sha.as_str());
        assert_eq!(v["actions"]["o/r@v1"]["tar_url"], format!("http://h/_actions/o_r/{sha}.tar.gz"));
        let (_, out) = route(
            "POST",
            "/actions/build/p/jobs/j/runnerresolve/actions",
            &tag,
            "http://h/",
            None,
            &BTreeMap::new(),
            None,
        )
        .unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&out).unwrap()["actions"], json!({}));
        assert_eq!(
            route("GET", path, &Value::Null, "", Some(&dir), &BTreeMap::new(), None).unwrap(),
            (200, b"tgz".to_vec())
        );
        // No other file of the directory, and no file with no directory.
        for bad in ["/_actions/../x.tar.gz", "/_actions/a/../../x", "/_actions/x.tar.gz", "/_actions/a/b/c"] {
            assert_eq!(
                route("GET", bad, &Value::Null, "", Some(&dir), &BTreeMap::new(), None).unwrap().0,
                404,
                "{bad}"
            );
        }
        assert_eq!(route("GET", path, &Value::Null, "", None, &BTreeMap::new(), None).unwrap().0, 404, "no directory");
        // T-708: each ref of `actions/checkout` is the stand-in, also with a
        // pin and a tarball of it in the directory.
        std::fs::create_dir_all(dir.join("actions_checkout")).unwrap();
        std::fs::write(dir.join(archive_name("actions/checkout", &sha)), b"real").unwrap();
        let pins = BTreeMap::from([("actions/checkout@v4".to_string(), sha.clone())]);
        let co = json!({ "actions": [{ "action": "actions/checkout", "version": "v4" },
            { "action": "actions/checkout", "version": sha }] });
        let (_, out) = route("POST", path_resolve(), &co, "http://h/", Some(&dir), &pins, None).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        for r in ["v4", sha.as_str()] {
            let info = &v["actions"][format!("actions/checkout@{r}")];
            assert_eq!(info["resolved_sha"], crate::stand_in::SHA, "{r}: {v}");
            let url = info["tar_url"].as_str().unwrap().trim_start_matches("http://h");
            let (status, got) = route("GET", url, &Value::Null, "", Some(&dir), &pins, None).unwrap();
            assert_eq!((status, got), (200, crate::stand_in::tarball()), "{r}: {url}");
        }
        assert!(route("GET", "/message", &Value::Null, "", None, &BTreeMap::new(), None).is_none());
        // The tree of the checkout, when the run gives one.
        let tree = dir.join("tree.tar.gz");
        std::fs::write(&tree, b"tree").unwrap();
        assert_eq!(
            route("GET", "/_checkout.tar.gz", &Value::Null, "", None, &BTreeMap::new(), Some(&tree)).unwrap(),
            (200, b"tree".to_vec())
        );
        assert_eq!(route("GET", "/_checkout.tar.gz", &Value::Null, "", None, &BTreeMap::new(), None).unwrap().0, 404);
        // The tree of a commit of the local git (T-640), beside it; no other
        // name of the directory.
        std::fs::write(dir.join(checkout_file(&sha)), b"at").unwrap();
        let get = |p: &str| route("GET", p, &Value::Null, "", None, &BTreeMap::new(), Some(&tree)).unwrap();
        assert_eq!(get(&format!("/{}", checkout_at(&sha))), (200, b"at".to_vec()));
        for bad in
            ["/_checkout/../tree.tar.gz", "/_checkout/aaaa.tar.gz", &format!("/_checkout/{}.tar.gz", "A".repeat(40))]
        {
            assert_eq!(get(bad).0, 404, "{bad}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-713: the route of a pinned tree for the stand-in of a checkout in
    /// an action; a key with no pin, a bad escape, a value that is no
    /// commit and a repository that is no name get 404.
    #[test]
    fn the_route_of_a_pinned_tree() {
        let (a, b) = ("a".repeat(40), "b".repeat(64));
        let pins = BTreeMap::from([
            (format!("{REF_PIN}v1"), a.clone()),
            (format!("{REF_PIN}refs/heads/main"), String::new()),
            (format!("{REF_PIN}gone"), "!`ref: gone` is not a commit".to_string()),
            (format!("{REF_PIN}bad"), "nocommit".to_string()),
            ("o/second@refs/tags/v1".to_string(), b.clone()),
            ("o/second@HEAD".to_string(), "sha256:0000".to_string()),
            ("../x@v1".to_string(), a.clone()),
            ("o/../x@v1".to_string(), a.clone()),
            ("o@v1".to_string(), a.clone()),
        ]);
        let get = |key: &str| {
            let (status, out) =
                route("GET", &format!("/{PIN}{key}"), &Value::Null, "", None, &pins, None).expect("a route");
            (status, serde_json::from_slice::<Value>(&out).unwrap_or(Value::Null))
        };
        assert_eq!(get("checkout-ref%3Av1"), (200, json!({ "route": checkout_at(&a) })));
        assert_eq!(get("checkout-ref:refs%2Fheads%2Fmain"), (200, json!({ "route": CHECKOUT })));
        assert_eq!(get("checkout-ref%3Agone"), (200, json!({ "fail": "`ref: gone` is not a commit" })));
        let other = json!({ "route": format!("_actions/o_second/{b}.tar.gz"), "strip": "1" });
        assert_eq!(get("o%2Fsecond%40refs%2Ftags%2Fv1"), (200, other));
        for bad in [
            "checkout-ref%3Anone",
            "checkout-ref%3Abad",
            "O%2Fsecond%40refs%2Ftags%2Fv1",
            "o%2Fsecond%40HEAD",
            "..%2Fx%40v1",
            "o%2F..%2Fx%40v1",
            "o%40v1",
            "checkout-ref%3",
            "checkout-ref%+3Av1",
            "checkout-ref%zzv1",
            "%FF",
            "",
        ] {
            assert_eq!(get(bad).0, 404, "{bad}");
        }
        // The stand-in sends the job token: the route needs it.
        assert_eq!(crate::auth::need("GET", &format!("/{PIN}x")), Some(crate::auth::Need::Either));
    }
}
