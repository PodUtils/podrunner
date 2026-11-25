//! The cache of `serve` (T-480, githome's G-07 and G-20, cfb's P-3): a host
//! directory that a unit mounts at [`MOUNT`], kept apart by source, forge,
//! repository, tier and key. The rules of GitHub for its own cache hold: a
//! job of the tier `untrusted` gets none; only a job of the events that
//! write the scope of the default branch on GitHub writes, on a ref of
//! `write_refs` (or when its source says `rw`) with no head of a fork; each
//! other job reads it.
//! The `cache` of an offer can only lower the mode.

mod store;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use podrunner_schema::offer::Event;
use podrunner_schema::{CacheMode, Fact, JobId, JobOffer, Residual, RunReport, Tier};
use serde::Deserialize;

use crate::engine::Slots;

pub use store::{Grant, Mode};

type Key = (usize, JobId);

/// Where a unit sees its cache.
pub const MOUNT: &str = "/podrunner-cache";
/// The longest key, in bytes.
pub const KEY_MAX: usize = 128;
const GIB: u64 = 1 << 30;

/// The table `[cache]`.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "RawCache")]
pub struct CacheConfig {
    pub dir: Option<PathBuf>,
    pub max_bytes: u64,
    pub total_bytes: u64,
    pub max_age_s: u64,
    pub write_refs: Vec<String>,
    pub key: String,
    pub env: BTreeMap<String, String>,
}

/// The keys of `[cache]` as the file gives them.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCache {
    /// The directory of the caches: an absolute path; default `STATE/cache`.
    #[serde(default)]
    dir: Option<PathBuf>,
    /// The most bytes of one cache; above it, the cache is emptied after
    /// its next `rw` job. Default 10 GiB.
    #[serde(default = "max_bytes")]
    max_bytes: u64,
    /// The most bytes of the directory; above it, the caches with the
    /// oldest use go. Default 50 GiB.
    #[serde(default = "total_bytes")]
    total_bytes: u64,
    /// A cache with no use in this time goes. Default 7 days.
    #[serde(default = "max_age_s")]
    max_age_s: u64,
    /// The refs whose jobs write the cache (the default branch of each
    /// repository). Default `["refs/heads/main"]`.
    #[serde(default = "write_refs")]
    write_refs: Vec<String>,
    /// The key of a job whose offer gives none. Default `default`.
    #[serde(default = "key")]
    key: String,
    /// A variable of the runner and a relative path under the cache, for
    /// example `npm_config_cache = "npm"`.
    #[serde(default)]
    env: BTreeMap<String, String>,
}

fn max_bytes() -> u64 {
    10 * GIB
}
fn total_bytes() -> u64 {
    50 * GIB
}
fn max_age_s() -> u64 {
    7 * 24 * 3600
}
fn write_refs() -> Vec<String> {
    vec!["refs/heads/main".into()]
}
fn key() -> String {
    "default".into()
}

impl TryFrom<RawCache> for CacheConfig {
    type Error = String;

    fn try_from(r: RawCache) -> Result<CacheConfig, String> {
        if let Some(d) = &r.dir {
            // `components` drops a `.` in the middle: read the text.
            let text = d.to_str().unwrap_or(":");
            let plain = text.split('/').all(|p| p != "." && p != "..");
            if !d.is_absolute() || !plain || text.contains(':') {
                return Err(format!("[cache] dir {d:?}: give an absolute path with no '.', '..' or ':'"));
            }
        }
        if r.max_bytes == 0 || r.max_age_s == 0 || r.total_bytes < r.max_bytes {
            return Err("[cache]: max_bytes and max_age_s are 1 or more, and total_bytes is max_bytes or more".into());
        }
        if let Some(w) = r.write_refs.iter().find(|w| !w.starts_with("refs/") || w.chars().any(char::is_whitespace)) {
            return Err(format!("[cache] write_refs: {w:?} is not a full ref (refs/heads/NAME)"));
        }
        if !key_ok(&r.key) {
            return Err(format!("[cache] key {:?}: 1 to {KEY_MAX} bytes with no control character", r.key));
        }
        for (name, path) in &r.env {
            env_ok(name, path).map_err(|e| format!("[cache] env {name:?}: {e}"))?;
        }
        Ok(CacheConfig {
            dir: r.dir,
            max_bytes: r.max_bytes,
            total_bytes: r.total_bytes,
            max_age_s: r.max_age_s,
            write_refs: r.write_refs,
            key: r.key,
            env: r.env,
        })
    }
}

/// A key of an offer or of the file: 1 to [`KEY_MAX`] bytes, no control
/// character. The scope hashes it, so `..` and `/` stay bytes.
pub fn key_ok(k: &str) -> bool {
    !k.is_empty() && k.len() <= KEY_MAX && !k.chars().any(char::is_control)
}

/// A variable of `[cache] env`: a name of the shell that does not set a
/// value of podrunner or of the runner, and a plain relative path.
fn env_ok(name: &str, path: &str) -> Result<(), &'static str> {
    let mut chars = name.chars();
    let shell = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    let upper = name.to_ascii_uppercase();
    if !shell || ["PODRUNNER_", "GITHUB_", "ACTIONS_"].iter().any(|p| upper.starts_with(p)) || upper == "PATH" {
        return Err("not a name of a variable, or a name that podrunner or the runner sets");
    }
    let p = Path::new(path);
    let parts = path.split('/').all(|p| !p.is_empty() && p != "." && p != "..");
    if path.len() > 255 || !parts || !p.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err("give a relative path with no '.' or '..'");
    }
    Ok(())
}

/// The events whose jobs write the scope of the default branch on GitHub.
fn writes(e: Event) -> bool {
    matches!(e, Event::Push | Event::WorkflowDispatch | Event::RepositoryDispatch | Event::Schedule)
}

/// The mode and the key of a job of `tier` on a kind that mounts (or not)
/// a host directory, or why it gets no cache.
pub fn decide(cfg: &CacheConfig, tier: Tier, offer: &JobOffer, mounts: bool) -> Result<(Mode, String), &'static str> {
    if tier == Tier::Untrusted {
        return Err("a job of the tier untrusted gets no cache");
    }
    if !mounts {
        return Err("the kind mounts no host directory (T-648)");
    }
    let asked = offer.cache.as_ref();
    let key = asked.map_or(cfg.key.as_str(), |c| c.key.as_str());
    if !key_ok(key) {
        return Err("the key of the offer is empty, longer than 128 bytes or has a control character");
    }
    let t = &offer.trust_inputs;
    let rw_asked = asked.is_some_and(|c| c.mode == CacheMode::ReadWrite);
    let mode = match asked.map(|c| c.mode) {
        Some(CacheMode::Off) => return Err("the offer asks for no cache"),
        Some(CacheMode::ReadOnly) => Mode::Ro,
        // A head of a fork never writes: the tier rule keeps a fork out of
        // `trusted`, and this holds also where no rule set the tier.
        _ if writes(t.event) && t.head_is_fork == Fact::No && (rw_asked || cfg.write_refs.contains(&t.git_ref)) => {
            Mode::Rw
        }
        _ => Mode::Ro,
    };
    Ok((mode, key.to_string()))
}

/// The scope of a cache, as the operator reads it, and its id: the first
/// 32 hex digits of its SHA-256. Each part has its length before it, so no
/// two scopes give the same bytes.
pub fn scope(source: &str, offer: &JobOffer, tier: Tier, key: &str) -> (String, String) {
    let t = &offer.trust_inputs;
    let forge = t.forge.as_deref().unwrap_or("github.com").to_ascii_lowercase();
    let tier = crate::config::tier_name(tier);
    let parts = [source, &forge, &t.owner.to_ascii_lowercase(), &t.repo.to_ascii_lowercase(), tier, key];
    let mut bytes = Vec::new();
    for p in parts {
        bytes.extend((p.len() as u64).to_le_bytes());
        bytes.extend(p.as_bytes());
    }
    let hash = ring::digest::digest(&ring::digest::SHA256, &bytes);
    let id: String = hash.as_ref()[..16].iter().map(|b| format!("{b:02x}")).collect();
    let text = serde_json::json!({
        "source": source, "forge": forge, "owner": parts[2], "repo": parts[3], "tier": tier, "key": key
    });
    (id, text.to_string() + "\n")
}

/// The cache bytes of the report of a job (G-19, G-20).
pub(crate) fn report(grant: Option<&Grant>, r: &mut RunReport) {
    let Some(g) = grant else {
        r.cache_read_bytes = Some(Residual::Count(0));
        r.cache_written_bytes = Some(Residual::Count(0));
        return;
    };
    r.cache_read_bytes = Some(Residual::Unknown(podrunner_schema::report::Unknown::Unknown));
    let why = "the host does not count the reads of a mounted cache (T-649)";
    r.unknown_reasons.insert("cache_read_bytes".into(), why.into());
    r.cache_written_bytes = Some(Residual::Count(g.written));
}

impl Slots {
    /// The cache of a placed job at its start: its mount, with its
    /// variables in `env`. None when the job gets no cache.
    pub(crate) fn cache_start(&mut self, key: &Key, env: &mut BTreeMap<String, String>) -> Vec<(PathBuf, String)> {
        let (Some(cfg), Some(j)) = (&self.cfg.cache, self.jobs.get(key)) else { return Vec::new() };
        let (Some(offer), Some(p), Some(unit)) = (&j.offer, &j.placement, &j.unit) else { return Vec::new() };
        let mounts = self.kinds[p.kind].driver.host_mounts();
        let (mut mode, k) = match decide(cfg, p.tier, offer, mounts) {
            Ok(d) => d,
            Err(why) => {
                eprintln!("podrunner: {unit}: no cache: {why}");
                return Vec::new();
            }
        };
        let (id, text) = scope(&self.sources[key.0].cfg.name, offer, p.tier, &k);
        // One writer of a cache at a time: the bytes of a job are its own.
        if mode == Mode::Rw && self.cache_in_use(key).any(|(i, m)| i == id && m == Mode::Rw) {
            mode = Mode::Ro;
        }
        let root = cfg.dir.clone().unwrap_or_else(|| self.state_dir.join("cache"));
        let (data, since) = match store::open(&root, &id, &text, cfg.env.values().map(String::as_str)) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("podrunner: {unit}: no cache: {}: {e}", root.display());
                return Vec::new();
            }
        };
        eprintln!("podrunner: {unit}: the cache {id} ({}), {}", mode.as_str(), text.trim_end());
        env.insert("PODRUNNER_CACHE".into(), MOUNT.into());
        env.insert("PODRUNNER_CACHE_MODE".into(), mode.as_str().into());
        // A variable that podrunner set before (the CA bundle of the
        // broker) stays.
        for (name, path) in &cfg.env {
            env.entry(name.clone()).or_insert_with(|| format!("{MOUNT}/{path}"));
        }
        let target = if mode == Mode::Ro { format!("{MOUNT}:ro") } else { MOUNT.to_string() };
        let grant = Grant { id, root, data: data.clone(), mode, since, written: 0 };
        if let Some(j) = self.jobs.get_mut(key) {
            j.times.cache = Some(grant);
        }
        vec![(data, target)]
    }

    /// The caches of the jobs other than `key` that have not ended.
    fn cache_in_use<'a>(&'a self, key: &'a Key) -> impl Iterator<Item = (String, Mode)> + 'a {
        let open = self.jobs.iter().filter(move |(k, j)| *k != key && !j.state.is_final());
        open.filter_map(|(_, j)| j.times.cache.as_ref().map(|g| (g.id.clone(), g.mode)))
    }

    /// After the destroy of the unit of `key`: the bytes that its job
    /// wrote, then the bound of the directory.
    pub(crate) fn cache_end(&mut self, key: &Key) {
        let Some(cfg) = self.cfg.cache.clone() else { return };
        let Some(g) = self.jobs.get(key).and_then(|j| j.times.cache.clone()) else { return };
        if g.mode != Mode::Rw {
            return;
        }
        let written = store::written_since(&g.data, g.since);
        if let Some(c) = self.jobs.get_mut(key).and_then(|j| j.times.cache.as_mut()) {
            c.written = written;
        }
        let used: std::collections::BTreeSet<String> = self.cache_in_use(key).map(|(id, _)| id).collect();
        let now = std::time::SystemTime::now();
        for (id, why) in store::bound(&g.root, &cfg, &used, &g.id, now) {
            eprintln!("podrunner: the cache {id} is removed: {why}");
        }
    }
}
