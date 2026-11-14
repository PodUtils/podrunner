//! The cache of the local mode (T-120): the CacheService of the results API
//! (twirp JSON, cache service v2) that `actions/cache@v4` and the cache of
//! the `setup-*` actions call at `ACTIONS_RESULTS_URL`, and the blob routes
//! of its signed URLs. The store stays across runs (`--cache DIR`).
//!
//! An entry is a key and a version (the client hashes its paths and its
//! compression into the version). A lookup takes the exact key first, then
//! each restore key as a prefix, newest first, and only entries of the same
//! version. An entry of a key and a version is written once. The store has a
//! limit of bytes: the oldest entry goes first. A download takes a `Range`,
//! as the concurrent download of the client asks.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::artifacts::{block_list, ok, query, random_hex, rfc3339, sha256};

const SERVICE: &str = "/twirp/github.actions.results.api.v1.CacheService/";
const BLOBS: &str = "/_cache/";

/// GitHub keeps 10 GB of cache for each repository.
pub const LIMIT: u64 = 10_000_000_000;

/// An answer with its extra headers.
pub type Answer = (u16, Vec<u8>, Vec<(&'static str, String)>);

struct Pending {
    key: String,
    version: String,
    sig: String,
}

#[derive(Default)]
struct Inner {
    next: u64,
    pending: BTreeMap<u64, Pending>,
}

/// The cache store.
pub struct CacheStore {
    dir: PathBuf,
    limit: u64,
    inner: Mutex<Inner>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The part of `len` bytes that a `Range: bytes=A-B` header names.
fn range(header: &str, len: u64) -> Option<(u64, u64)> {
    let (a, b) = header.trim().strip_prefix("bytes=")?.split_once('-')?;
    let (start, end) = match (a.trim(), b.trim()) {
        ("", n) => (len.checked_sub(n.parse().ok()?)?, len.checked_sub(1)?),
        (s, "") => (s.parse().ok()?, len.checked_sub(1)?),
        (s, e) => (s.parse().ok()?, e.parse::<u64>().ok()?.min(len.checked_sub(1)?)),
    };
    (start <= end && end < len).then_some((start, end))
}

impl CacheStore {
    pub fn new(dir: &Path, limit: u64) -> std::io::Result<CacheStore> {
        fs::create_dir_all(dir.join(".pending"))?;
        fs::create_dir_all(dir.join("entries"))?;
        Ok(CacheStore { dir: dir.to_path_buf(), limit, inner: Mutex::default() })
    }

    /// The directory of an entry: a digest of its key and version, so no key
    /// is a path.
    fn entry_dir(&self, key: &str, version: &str) -> PathBuf {
        let d = sha256(format!("{key}\n{version}").as_bytes());
        self.dir.join("entries").join(&d["sha256:".len().."sha256:".len() + 32])
    }

    /// Each entry of the store, newest first.
    pub fn entries(&self) -> Vec<Value> {
        let mut out: Vec<Value> = fs::read_dir(self.dir.join("entries"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| fs::read_to_string(e.path().join("entry.json")).ok())
            .filter_map(|t| serde_json::from_str(&t).ok())
            .collect();
        out.sort_by_key(|e: &Value| std::cmp::Reverse((e["created"].as_u64(), e["id"].as_str().map(str::to_string))));
        out
    }

    /// The entry for a key and its restore keys, of one version.
    fn lookup(&self, key: &str, restore: &[&str], version: &str) -> Option<Value> {
        let all: Vec<Value> = self.entries().into_iter().filter(|e| e["version"] == version).collect();
        if let Some(e) = all.iter().find(|e| e["key"] == key) {
            return Some(e.clone());
        }
        restore.iter().find_map(|p| all.iter().find(|e| e["key"].as_str().is_some_and(|k| k.starts_with(p))).cloned())
    }

    pub fn route(&self, method: &str, path: &str, body: &[u8], header: Option<&str>, base: &str) -> Option<Answer> {
        let (p, q) = path.split_once('?').unwrap_or((path, ""));
        if let Some(call) = p.strip_prefix(SERVICE) {
            let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
            let (status, out) = if method == "POST" { self.call(call, &v, base) } else { (405, Vec::new()) };
            return Some((status, out, Vec::new()));
        }
        let (kind, id) = p.strip_prefix(BLOBS)?.split_once('/')?;
        let sig = query(q, "sig").unwrap_or_default();
        Some(match (method, kind) {
            ("PUT", "upload") => {
                let (s, b) = self.put(id.parse().ok()?, &sig, q, body);
                (s, b, Vec::new())
            }
            ("GET", "download") => self.download(id, &sig, header),
            _ => (404, Vec::new(), Vec::new()),
        })
    }

    fn call(&self, call: &str, v: &Value, base: &str) -> (u16, Vec<u8>) {
        let key = v["key"].as_str().unwrap_or("");
        let version = v["version"].as_str().unwrap_or("");
        match call {
            "CreateCacheEntry" => {
                if key.is_empty() || key.len() > 512 || key.contains(',') || version.is_empty() {
                    return ok(
                        json!({ "ok": false, "message": format!("the key {key:?} or its version is not valid") }),
                    );
                }
                if self.entry_dir(key, version).join("entry.json").is_file() {
                    let message = format!("a cache entry of the key {key:?} and its version exists");
                    return ok(json!({ "ok": false, "message": message }));
                }
                let mut st = self.inner.lock().expect("the cache");
                st.pending.retain(|_, p| p.key != key || p.version != version);
                st.next += 1;
                let id = st.next;
                let sig = random_hex();
                let dir = self.dir.join(".pending").join(id.to_string());
                let _ = fs::remove_dir_all(&dir);
                if fs::create_dir_all(dir.join("blocks")).is_err() {
                    return ok(json!({ "ok": false, "message": "the store cannot write" }));
                }
                st.pending.insert(id, Pending { key: key.into(), version: version.into(), sig: sig.clone() });
                ok(json!({ "ok": true, "signed_upload_url": format!("{base}_cache/upload/{id}?sig={sig}") }))
            }
            "FinalizeCacheEntryUpload" => self.finalize(key, version, v),
            "GetCacheEntryDownloadURL" => {
                let restore: Vec<&str> = v["restore_keys"]
                    .as_array()
                    .map(|a| a.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                match self.lookup(key, &restore, version) {
                    Some(e) => {
                        let (id, sig) = (e["id"].as_str().unwrap_or(""), e["sig"].as_str().unwrap_or(""));
                        ok(json!({
                            "ok": true, "matched_key": e["key"],
                            "signed_download_url": format!("{base}_cache/download/{id}?sig={sig}"),
                        }))
                    }
                    None => ok(json!({ "ok": false, "signed_download_url": "", "matched_key": "" })),
                }
            }
            _ => (404, json!({ "code": "bad_route", "msg": format!("no method {call}") }).to_string().into_bytes()),
        }
    }

    fn put(&self, id: u64, sig: &str, q: &str, body: &[u8]) -> (u16, Vec<u8>) {
        let st = self.inner.lock().expect("the cache");
        if st.pending.get(&id).is_none_or(|p| p.sig != sig) {
            return (403, Vec::new());
        }
        if body.len() as u64 > self.limit {
            return (413, Vec::new());
        }
        let dir = self.dir.join(".pending").join(id.to_string());
        let name = |b: &str| b.bytes().map(|x| format!("{x:02x}")).collect::<String>();
        let r = match query(q, "comp").unwrap_or_default().as_str() {
            "block" => match query(q, "blockid") {
                Some(b) => fs::write(dir.join("blocks").join(name(&b)), body),
                None => return (400, Vec::new()),
            },
            "blocklist" => {
                let mut all = Vec::new();
                for b in block_list(&String::from_utf8_lossy(body)) {
                    match fs::read(dir.join("blocks").join(name(&b))) {
                        Ok(bytes) => all.extend_from_slice(&bytes),
                        Err(_) => return (400, format!("no block {b}").into_bytes()),
                    }
                }
                fs::write(dir.join("blob"), all)
            }
            "" => fs::write(dir.join("blob"), body),
            _ => return (400, Vec::new()),
        };
        match r {
            Ok(()) => (201, Vec::new()),
            Err(e) => (500, e.to_string().into_bytes()),
        }
    }

    fn finalize(&self, key: &str, version: &str, v: &Value) -> (u16, Vec<u8>) {
        let mut st = self.inner.lock().expect("the cache");
        let found = st.pending.iter().find(|(_, p)| p.key == key && p.version == version).map(|(i, _)| *i);
        let Some(id) = found else {
            return ok(json!({ "ok": false, "message": format!("no upload of the key {key:?}") }));
        };
        let dir = self.dir.join(".pending").join(id.to_string());
        let Ok(blob) = fs::read(dir.join("blob")) else {
            return ok(json!({ "ok": false, "message": format!("the upload of {key:?} has no blob") }));
        };
        let size = v["size_bytes"].as_str().and_then(|s| s.parse().ok()).or(v["size_bytes"].as_u64());
        if size != Some(blob.len() as u64) {
            let message = format!("{key:?}: size {size:?}, the blob has {}", blob.len());
            return ok(json!({ "ok": false, "message": message }));
        }
        let pending = st.pending.remove(&id).expect("a pending upload");
        let out = self.entry_dir(key, version);
        let entry_id = format!("{}{id:06}", now());
        let meta = json!({
            "id": entry_id, "key": key, "version": version, "size": blob.len(), "created": now(),
            "created_at": rfc3339(now()), "digest": sha256(&blob), "sig": pending.sig,
        });
        let r = fs::create_dir_all(&out)
            .and_then(|_| fs::rename(dir.join("blob"), out.join("blob")))
            .and_then(|_| fs::write(out.join("entry.json"), meta.to_string()));
        let _ = fs::remove_dir_all(&dir);
        if let Err(e) = r {
            return ok(json!({ "ok": false, "message": e.to_string() }));
        }
        self.evict();
        ok(json!({ "ok": true, "entry_id": entry_id }))
    }

    /// Remove the oldest entries until the store is within its limit.
    fn evict(&self) {
        let mut entries = self.entries();
        let mut total: u64 = entries.iter().filter_map(|e| e["size"].as_u64()).sum();
        while total > self.limit {
            let Some(old) = entries.pop() else { break };
            let (k, v) = (old["key"].as_str().unwrap_or(""), old["version"].as_str().unwrap_or(""));
            let _ = fs::remove_dir_all(self.entry_dir(k, v));
            total -= old["size"].as_u64().unwrap_or(0);
        }
    }

    fn download(&self, id: &str, sig: &str, header: Option<&str>) -> Answer {
        let Some(e) = self.entries().into_iter().find(|e| e["id"] == id) else { return (404, Vec::new(), Vec::new()) };
        if e["sig"] != sig {
            return (403, Vec::new(), Vec::new());
        }
        let (k, v) = (e["key"].as_str().unwrap_or(""), e["version"].as_str().unwrap_or(""));
        let Ok(blob) = fs::read(self.entry_dir(k, v).join("blob")) else { return (404, Vec::new(), Vec::new()) };
        let len = blob.len() as u64;
        let ranges = vec![("Accept-Ranges", "bytes".to_string())];
        match header {
            None => (200, blob, ranges),
            Some(h) => match range(h, len) {
                Some((a, b)) => {
                    let mut headers = ranges;
                    headers.push(("Content-Range", format!("bytes {a}-{b}/{len}")));
                    (206, blob[a as usize..=b as usize].to_vec(), headers)
                }
                None => (416, Vec::new(), vec![("Content-Range", format!("bytes */{len}"))]),
            },
        }
    }
}

#[cfg(test)]
mod tests;
