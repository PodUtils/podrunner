//! The cache of `serve` (T-480, githome's G-07 and G-20, cfb's P-3): a host
//! directory that a unit mounts at [`MOUNT`], kept apart by source, forge,
//! repository, tier and key. The rules of GitHub for its own cache hold: a
//! job of the tier `untrusted` gets none; only a job of the events that
//! write the scope of the default branch on GitHub writes, on a ref of
//! `write_refs` (or when its source says `rw`) with no head of a fork; each
//! other job reads it.
//! The `cache` of an offer can only lower the mode.

mod slots;
mod store;
#[cfg(test)]
mod tests;
mod worker;

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use podrunner_schema::offer::{Event, TrustInputs};
use podrunner_schema::{CacheMode, Fact, JobId, JobOffer, Residual, RunReport, Tier};
use serde::Deserialize;

pub use store::{Grant, Mode};
pub(crate) use worker::Worker;

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
    /// repository). Default `["refs/heads/main"]`: a branch named `main`,
    /// also where the default branch is another one.
    #[serde(default = "write_refs")]
    write_refs: Vec<String>,
    /// The key of a job whose offer gives none. Default `default`.
    #[serde(default = "key")]
    key: String,
    /// A variable of the runner and a relative path under the cache, for
    /// example `npm_config_cache = "npm"`. Only a `rw` job gets them. A name
    /// that loads code (`LD_*`, `NODE_OPTIONS`) or that podrunner or the
    /// runner sets is refused; `RUNNER_TOOL_CACHE` is allowed.
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

/// The names that `[cache] env` refuses: each loads code or settings into
/// the processes of a job, or is a value of the runner. A cache is written
/// by jobs, so it is no place for them.
const DENIED: [&str; 21] = [
    "PATH",
    "HOME",
    "SHELL",
    "ENV",
    "BASH_ENV",
    "IFS",
    "PS4",
    "PROMPT_COMMAND",
    "TMPDIR",
    "NODE_OPTIONS",
    "NODE_PATH",
    "JAVA_TOOL_OPTIONS",
    "_JAVA_OPTIONS",
    "JDK_JAVA_OPTIONS",
    "PYTHONPATH",
    "PYTHONSTARTUP",
    "PYTHONHOME",
    "PERL5LIB",
    "PERL5OPT",
    "RUBYOPT",
    "RUBYLIB",
];
const DENIED_PREFIX: [&str; 6] = ["PODRUNNER_", "GITHUB_", "ACTIONS_", "RUNNER_", "LD_", "GIT_"];

/// A variable of `[cache] env`: a name of the shell that does not set a
/// value of podrunner or of the runner, and a plain relative path.
fn env_ok(name: &str, path: &str) -> Result<(), &'static str> {
    let mut chars = name.chars();
    let shell = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    let upper = name.to_ascii_uppercase();
    // The runner reads its tool cache from this variable (githome's G-20).
    let tool_cache = upper == "RUNNER_TOOL_CACHE";
    let denied = DENIED.contains(&upper.as_str()) || DENIED_PREFIX.iter().any(|p| upper.starts_with(p));
    if !shell || (denied && !tool_cache) {
        return Err("not a name of a variable, or a name that loads code or that podrunner or the runner sets");
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
    let asked = offer.cache.as_ref();
    let key = asked.map_or(cfg.key.as_str(), |c| c.key.as_str());
    decide_facts(cfg, tier, &offer.trust_inputs, key, asked.map(|c| c.mode), mounts)
}

/// [`decide`] on the facts `t`, the key `key` and the mode that a source
/// asked (none: the rule alone).
fn decide_facts(
    cfg: &CacheConfig,
    tier: Tier,
    t: &TrustInputs,
    key: &str,
    asked: Option<CacheMode>,
    mounts: bool,
) -> Result<(Mode, String), &'static str> {
    if tier == Tier::Untrusted {
        return Err("a job of the tier untrusted gets no cache");
    }
    if !mounts {
        return Err("the kind mounts no host directory (T-648)");
    }
    if !key_ok(key) {
        return Err("the key of the offer is empty, longer than 128 bytes or has a control character");
    }
    let rw_asked = asked == Some(CacheMode::ReadWrite);
    let mode = match asked {
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

/// Whether the cache `grant` of a job of `offer` holds for the real job
/// of `facts`, when the forge gave the runner a job of another run
/// (T-493): its scope is the same and its mode is not lower. The `rw` of
/// an offer is the word of the source on the run of the offer only.
pub fn holds(
    cfg: &CacheConfig,
    grant: &Grant,
    source: &str,
    offer: &JobOffer,
    facts: &TrustInputs,
    tier: Tier,
    mounts: bool,
) -> bool {
    let asked = offer.cache.as_ref();
    let key = asked.map_or(cfg.key.as_str(), |c| c.key.as_str());
    let mode = asked.map(|c| c.mode).filter(|m| *m != CacheMode::ReadWrite);
    match decide_facts(cfg, tier, facts, key, mode, mounts) {
        Ok((m, k)) => !(grant.mode == Mode::Rw && m == Mode::Ro) && scope_of(source, facts, tier, &k).0 == grant.id,
        Err(_) => false,
    }
}

/// The scope of a cache, as the operator reads it, and its id: the first
/// 32 hex digits of its SHA-256. Each part has its length before it, so no
/// two scopes give the same bytes.
pub fn scope(source: &str, offer: &JobOffer, tier: Tier, key: &str) -> (String, String) {
    scope_of(source, &offer.trust_inputs, tier, key)
}

/// [`scope`] on the facts `t`. The id of the repository, when the source
/// gives it, is a part: a repository made again with the same name does
/// not get the cache of the old one.
fn scope_of(source: &str, t: &TrustInputs, tier: Tier, key: &str) -> (String, String) {
    let forge = t.forge.as_deref().unwrap_or("github.com").to_ascii_lowercase();
    let tier = crate::config::tier_name(tier);
    let repo_id = t.repo_id.map(|n| n.to_string()).unwrap_or_default();
    let parts = [source, &forge, &t.owner.to_ascii_lowercase(), &t.repo.to_ascii_lowercase(), &repo_id, tier, key];
    let mut bytes = Vec::new();
    for p in parts {
        bytes.extend((p.len() as u64).to_le_bytes());
        bytes.extend(p.as_bytes());
    }
    let hash = ring::digest::digest(&ring::digest::SHA256, &bytes);
    let id: String = hash.as_ref()[..16].iter().map(|b| format!("{b:02x}")).collect();
    let text = serde_json::json!({
        "source": source, "forge": forge, "owner": parts[2], "repo": parts[3], "repo_id": t.repo_id,
        "tier": tier, "key": key
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
