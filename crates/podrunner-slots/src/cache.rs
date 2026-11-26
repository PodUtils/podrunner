//! The cache of `serve` (T-480, githome's G-07 and G-20, cfb's P-3): a host
//! directory that a unit mounts at [`MOUNT`], kept apart by source, forge,
//! repository, tier and key. The rules of GitHub for its own cache hold: a
//! job of the tier `untrusted` gets none; only a job of the events that
//! write the scope of the default branch on GitHub writes, on a ref of
//! `write_refs` (or when its source says `rw`) with no head of a fork; each
//! other job reads it, on a kind that proves a read-only mount, and gets
//! none on another kind. A writer reads only, on a kind that does not list
//! `rw` (T-648). The `cache` of an offer can only lower the mode.

mod slots;
mod store;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_mounts;
mod worker;

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use podrunner_schema::offer::{Event, TrustInputs};
use podrunner_schema::{CacheMode, Fact, JobId, JobOffer, Residual, RunReport, Tier};
use serde::Deserialize;

pub use store::{Grant, Mode, Uses};
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
    /// example `npm_config_cache = "npm"`. Only a `rw` job gets them. Only
    /// a known cache variable is taken (`npm_config_cache`,
    /// `PLAYWRIGHT_BROWSERS_PATH`, `RUNNER_TOOL_CACHE` and more).
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

/// The names that `[cache] env` takes: each names only the cache
/// directory of a tool. A name that makes a tool load its settings or
/// code from a directory (`XDG_CONFIG_HOME`, `CARGO_HOME`, `GEM_HOME`,
/// `GRADLE_USER_HOME`, `LD_*`, `CCACHE_DIR` with its `ccache.conf`) is not
/// a cache, so no list of refused names is complete. A tool reads the name in this case only.
pub const ENV_NAMES: [&str; 20] = [
    "npm_config_cache",
    "NPM_CONFIG_CACHE",
    "YARN_CACHE_FOLDER",
    "PNPM_STORE_DIR",
    "BUN_INSTALL_CACHE_DIR",
    "PLAYWRIGHT_BROWSERS_PATH",
    "PUPPETEER_CACHE_DIR",
    "CYPRESS_CACHE_FOLDER",
    "ELECTRON_CACHE",
    // The runner reads its tool cache from it (githome's G-20).
    "RUNNER_TOOL_CACHE",
    "PIP_CACHE_DIR",
    "UV_CACHE_DIR",
    "POETRY_CACHE_DIR",
    "GOMODCACHE",
    "GOCACHE",
    "SCCACHE_DIR",
    "DENO_DIR",
    "COMPOSER_CACHE_DIR",
    "NUGET_PACKAGES",
    "ZIG_GLOBAL_CACHE_DIR",
];

/// A variable of `[cache] env`: a name of [`ENV_NAMES`] and a plain
/// relative path.
fn env_ok(name: &str, path: &str) -> Result<(), &'static str> {
    if !ENV_NAMES.contains(&name) {
        return Err("not a cache variable that podrunner knows (docs/configuration.md lists them)");
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

/// What a kind proves of the host directories of a start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mounts {
    /// The kind mounts a host directory (`UnitDriver::host_mounts`).
    pub host: bool,
    /// The kind holds a `:ro` mount read-only: its capability lists the
    /// mode `ro` in `mount_modes`.
    pub ro: bool,
    /// The user of the unit writes a mount, and the user of podrunner
    /// owns or can remove each file that it writes: the capability lists
    /// `rw`. Else a job that would write gets `ro` (T-648: on the microvm
    /// and incus kinds a file of the guest user is not the host user's).
    pub rw: bool,
}

impl Mounts {
    /// The mounts of a kind with the driver `driver` and the record `cap`.
    pub fn of(driver: &dyn podrunner_unit::UnitDriver, cap: &podrunner_schema::Capability) -> Mounts {
        let has = |mode: &str| cap.mount_modes.iter().any(|m| m == mode);
        Mounts { host: driver.host_mounts(), ro: has("ro"), rw: has("rw") }
    }
}

/// Why a job gets no cache: a code for the audit line, and a text for the
/// operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoCache {
    pub code: &'static str,
    pub why: &'static str,
}

/// A `ro` job on a kind with no proven read-only mount: its writes would
/// reach the cache that the next `rw` job, of a higher trust, reads.
pub(crate) const NO_RO_MOUNT: NoCache = NoCache {
    code: "no-ro-mount",
    why: "the job would get ro, and the kind does not prove a read-only mount (mount_modes has no ro)",
};

/// The mode and the key of a job of `tier` on a kind with `mounts`, or
/// why it gets no cache.
pub fn decide(cfg: &CacheConfig, tier: Tier, offer: &JobOffer, mounts: Mounts) -> Result<(Mode, String), NoCache> {
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
    mounts: Mounts,
) -> Result<(Mode, String), NoCache> {
    let no = |code, why| Err(NoCache { code, why });
    if tier == Tier::Untrusted {
        return no("untrusted", "a job of the tier untrusted gets no cache");
    }
    if !mounts.host {
        return no("no-mount", "the kind mounts no host directory (T-648)");
    }
    if !key_ok(key) {
        return no("bad-key", "the key of the offer is empty, longer than 128 bytes or has a control character");
    }
    let rw_asked = asked == Some(CacheMode::ReadWrite);
    let mode = match asked {
        Some(CacheMode::Off) => return no("offer-off", "the offer asks for no cache"),
        Some(CacheMode::ReadOnly) => Mode::Ro,
        // A head of a fork never writes: the tier rule keeps a fork out of
        // `trusted`, and this holds also where no rule set the tier.
        _ if writes(t.event) && t.head_is_fork == Fact::No && (rw_asked || cfg.write_refs.contains(&t.git_ref)) => {
            Mode::Rw
        }
        _ => Mode::Ro,
    };
    // A kind with no `rw` gives a writer the cache to read.
    let mode = if mounts.rw { mode } else { Mode::Ro };
    if mode == Mode::Ro && !mounts.ro {
        return Err(NO_RO_MOUNT);
    }
    Ok((mode, key.to_string()))
}

/// Whether the cache `grant` of a job of `offer` holds for the real job
/// `real` of the facts `facts`, when the forge gave the runner a job of
/// another run (T-493). Each grant needs the event of the hook equal to
/// the event of the facts, no head of a fork from the hook, and the same
/// scope from the facts. A `rw` grant also needs `hooked` (the payload of
/// the unit carries the job-started hook: on a unit with no hook, a step
/// asks first with the facts that it chooses, T-663), the ref of the hook
/// equal to the ref of the facts, a ref that writes (in `write_refs`, or
/// the source said `rw` for that ref), and the repository id of the hook
/// equal to the one of the facts and of the grant (T-653). A mount is not
/// lowered after the start, so a `rw` grant that does not hold ends the
/// job.
#[allow(clippy::too_many_arguments)]
pub fn holds(
    cfg: &CacheConfig,
    grant: &Grant,
    source: &str,
    offer: &JobOffer,
    real: &podrunner_source::RealJob,
    facts: &TrustInputs,
    tier: Tier,
    mounts: Mounts,
    hooked: bool,
) -> bool {
    if real.head_is_fork.is_some_and(|f| f) {
        return false;
    }
    let event = serde_json::to_value(facts.event).ok();
    if event.as_ref().and_then(|v| v.as_str()) != Some(real.event.as_str()) {
        return false;
    }
    let asked = offer.cache.as_ref();
    let key = asked.map_or(cfg.key.as_str(), |c| c.key.as_str());
    let mut mode = asked.map(|c| c.mode).filter(|m| *m != CacheMode::ReadWrite);
    if grant.mode == Mode::Rw {
        if !hooked || !same_ref_and_repository(offer, real, facts) {
            return false;
        }
        // The `rw` of the source names the ref of its offer only.
        if asked.is_some_and(|c| c.mode == CacheMode::ReadWrite) && offer.trust_inputs.git_ref == facts.git_ref {
            mode = Some(CacheMode::ReadWrite);
        }
    }
    match decide_facts(cfg, tier, facts, key, mode, mounts) {
        Ok((m, k)) => (grant.mode == Mode::Ro || m == Mode::Rw) && scope_of(source, facts, tier, &k).0 == grant.id,
        Err(_) => false,
    }
}

/// The hook gives a ref and a repository id, the ref is the ref of the
/// facts, and the id is the one of the facts and of the offer (the grant).
fn same_ref_and_repository(offer: &JobOffer, real: &podrunner_source::RealJob, facts: &TrustInputs) -> bool {
    let (Some(r), Some(id)) = (real.git_ref.as_deref(), real.repo_id) else { return false };
    r == facts.git_ref && facts.repo_id == Some(id) && offer.trust_inputs.repo_id == Some(id)
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
    let unknown = || Some(Residual::Unknown(podrunner_schema::report::Unknown::Unknown));
    r.cache_read_bytes = unknown();
    let why = "the host does not count the reads of a mounted cache (T-649)";
    r.unknown_reasons.insert("cache_read_bytes".into(), why.into());
    match g.written {
        Some(n) => r.cache_written_bytes = Some(Residual::Count(n)),
        None => {
            r.cache_written_bytes = unknown();
            let why = "the thread of the caches gave no count in [deadlines] report_s after the destroy";
            r.unknown_reasons.insert("cache_written_bytes".into(), why.into());
        }
    }
}
