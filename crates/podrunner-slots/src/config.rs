//! `podrunner.toml` (`docs/configuration.md`). A file that lowers the
//! floor of a tier, or that names a `fake` part with no `allow_fake`, is
//! refused. An unknown key is refused.

use std::collections::{BTreeMap, BTreeSet};

use podrunner_schema::{Boundary, Tier};
use podrunner_tier::TierConfig;
use serde::Deserialize;
use serde_json::Value;

pub use crate::policy::EgressConfig;
pub use crate::unit_config::{HOSTED_WORK_FOLDER, RUNNER_HOME, UnitConfig, UnitInit, WarmCount, init_name};

/// The kind of a source.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    Fake,
    GithubScaleset,
    /// A remote mint over mutual TLS (GitHub #31): the source and its
    /// credentials live on another machine.
    Remote,
    /// A program that speaks the source wire on its stdio (T-101, G-01).
    Program,
}

/// One source instance: one set of one tier, one architecture, one host.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConfig {
    pub name: String,
    pub kind: SourceKind,
    pub tier: Tier,
    pub arch: String,
    /// True when the operator controls the source: it gets the full run
    /// report.
    #[serde(default)]
    pub operator_controlled: bool,
    /// The script of a `fake` source.
    #[serde(default)]
    pub script: Option<String>,
    /// The most units of this source at one time (no limit when absent).
    #[serde(default)]
    pub slots: Option<u32>,
    /// `github-scaleset`: the sidecar, the organization URL, the REST API,
    /// the installation and the App, the scale set, and the file of the
    /// private key of the App (mode 0400 or 0600).
    #[serde(default)]
    pub sidecar: Option<String>,
    #[serde(default)]
    pub config_url: Option<String>,
    #[serde(default)]
    pub api_url: Option<String>,
    #[serde(default)]
    pub installation_id: Option<u64>,
    #[serde(default)]
    pub app_id: Option<String>,
    #[serde(default)]
    pub scale_set: Option<String>,
    #[serde(default)]
    pub key_file: Option<String>,
    /// `github-scaleset`: a fine-grained token of the organization (mode
    /// 0400 or 0600) in place of the App (T-174).
    #[serde(default)]
    pub token_file: Option<String>,
    /// `github-scaleset`: serve a repository of a personal account
    /// (`config_url` names the repository; T-504, the owner's Q13). Off by
    /// default: the sidecar refuses a personal account (`repo-scope-refused`,
    /// call C3). The App needs "Administration: write" on the repository.
    #[serde(default)]
    pub allow_personal_account: bool,
    /// `remote`: the address of the mint (`host:port`), the pin of its key
    /// (`sha256/BASE64`), and the certificate of this host (its key is
    /// `key_file`, a P-256 key at mode 0600).
    #[serde(default)]
    pub mint: Option<String>,
    #[serde(default)]
    pub mint_pin: Option<String>,
    #[serde(default)]
    pub cert_file: Option<String>,
    /// `program`: the program, in `driver_dir`, and its argv. Its credential
    /// file is `key_file` (optional, mode 0400 or 0600): the program gets
    /// its path in `PODRUNNER_SOURCE_KEY_FILE`, never its content.
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub program_args: Vec<String>,
    /// The trust lists of this source, in place of `[trust]` (T-494).
    #[serde(default)]
    pub trust: Option<crate::trust::SourceTrust>,
}

/// `[dns]`: the resolver of podrunner (`pod-dns`): the system, the cache of
/// addresses that carried a session, DNS over HTTPS to resolvers by their
/// address. A host whose own resolver fails keeps its egress and its mint.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DnsConfig {
    /// Ask DNS over HTTPS when the system and the cache give no address.
    #[serde(default = "yes")]
    pub doh: bool,
    /// `HOST=IP` pins: a pinned name is not asked of the system.
    #[serde(default)]
    pub pins: Vec<String>,
}

impl Default for DnsConfig {
    fn default() -> Self {
        DnsConfig { doh: true, pins: Vec::new() }
    }
}

fn yes() -> bool {
    true
}

/// The floor of one tier.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierFloor {
    pub min_boundary: Boundary,
}

/// The time limit of each state, in seconds.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Deadlines {
    pub prepare_s: u64,
    pub claim_s: u64,
    pub mint_s: u64,
    pub start_s: u64,
    pub running_s: u64,
    pub grace_s: u64,
    pub destroy_s: u64,
    pub report_s: u64,
}

impl Default for Deadlines {
    fn default() -> Self {
        Deadlines {
            prepare_s: 600,
            claim_s: 60,
            mint_s: 60,
            start_s: 120,
            running_s: 6 * 3600,
            grace_s: 30,
            destroy_s: 60,
            report_s: 60,
        }
    }
}

/// A pinned payload of one architecture.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub arch: String,
    /// `NAME@sha256:HEX`.
    pub reference: String,
    /// The time of the build, in seconds of the Unix epoch.
    pub built_at_s: u64,
    /// The version of the official runner in the image (`2.338.2`).
    #[serde(default)]
    pub runner_version: Option<String>,
    /// The runner hook requests an assignment verdict before any job step.
    #[serde(default)]
    pub assignment_hook: bool,
    /// The payload carries the Docker engine binaries for a guest.
    #[serde(default)]
    pub container_engine: bool,
    /// The age limit of this pin; the smaller of it and `max_age_s` of
    /// `[pin]` holds.
    #[serde(default)]
    pub max_age_s: Option<u64>,
    /// The image of a sandbox (T-506): only an offer with `sandbox` of
    /// this digest takes it, and a runner job never does.
    #[serde(default)]
    pub sandbox: bool,
    /// The bytes of the image unpacked (its tree, `du -sxb /` in a unit).
    /// `check` refuses a pin above the root disk of a kind that runs it
    /// (T-551) or above `(mem - 512 MiB) × 4 / 13` of a microvm guest
    /// (T-573), and placement refuses an offer whose `disk` limit is below it
    /// on a kind with `root_quota` (T-571). Absent: not checked.
    #[serde(default)]
    pub unpacked_bytes: Option<u64>,
    /// The `runs-on` labels that this payload serves (no case). An offer
    /// with one of them takes a payload that names it, not one with no
    /// labels; empty: each offer (T-549).
    #[serde(default)]
    pub labels: Vec<String>,
    /// The PID 1 that the payload needs: `image` for an image that boots
    /// its own init (systemd). Placement sends it only to a kind with the
    /// same `init` (T-549).
    #[serde(default)]
    pub init: Option<UnitInit>,
}

impl Payload {
    /// True when the image is older than its limit at `now_s`, or built
    /// after `now_s` by more than a skew of the clocks: a wrong time would
    /// keep it young for ever (#287).
    pub fn stale(&self, pins: &Pins, now_s: u64) -> bool {
        let limit = self.max_age_s.map_or(pins.max_age_s, |m| m.min(pins.max_age_s));
        self.built_at_s > now_s.saturating_add(crate::pins::SKEW_S) || now_s.saturating_sub(self.built_at_s) > limit
    }

    /// True when the payload names one of `labels` (GitHub compares a
    /// label with no case).
    pub fn names_label(&self, labels: &[String]) -> bool {
        self.labels.iter().any(|l| labels.iter().any(|o| o.eq_ignore_ascii_case(l)))
    }
}

impl Pins {
    /// The payload engine digests of one architecture.
    pub fn payload_engine_digests(&self, arch: &str) -> BTreeSet<String> {
        self.payload
            .iter()
            .filter(|p| p.arch == arch && p.container_engine)
            .filter_map(|p| podrunner_unit::parse_pinned(&p.reference).ok().map(|(_, digest)| digest.to_string()))
            .collect()
    }

    /// The first payload of `arch` that a kind with `init` runs: the image
    /// of the self-tests of that kind (T-549).
    pub fn first_for(&self, arch: &str, init: Option<UnitInit>) -> Option<&Payload> {
        self.payload.iter().find(|p| p.arch == arch && p.init == init)
    }
}

/// The pin list.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pins {
    /// An image older than this is `image-stale`.
    #[serde(default = "thirty_days")]
    pub max_age_s: u64,
    #[serde(default)]
    pub payload: Vec<Payload>,
    /// The Ed25519 key (64 hex digits) that signs the pin list, and its
    /// signature (128 hex digits): `podrunner pin-sign` (DR-4).
    #[serde(default)]
    pub signer: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
}

fn thirty_days() -> u64 {
    30 * 24 * 3600
}

fn microvm() -> Vec<String> {
    vec!["microvm".into()]
}

fn hour() -> u64 {
    3600
}

/// The whole file.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The name of the host in the full report.
    pub host: String,
    #[serde(default)]
    pub allow_fake: bool,
    /// A terminal refusal suppresses its job id for this time.
    #[serde(default = "hour")]
    pub terminal_suppress_s: u64,
    /// More than one unit at a time on a shared principal.
    #[serde(default)]
    pub same_principal_ok: bool,
    /// Take the payload of an offer when it is on the pin list.
    #[serde(default)]
    pub payload_from_offer: bool,
    /// The directory of the driver executables (the podvm binary, the
    /// sidecars): each one must be in it (GitHub #36).
    #[serde(default)]
    pub driver_dir: Option<String>,
    /// The CONNECT proxy of the way out of the egress module (`http://host:port`);
    /// absent: direct.
    #[serde(default)]
    pub upstream_proxy: Option<String>,
    /// The upstream proxy takes names only: the broker sends it the checked
    /// name, and checks at the start that it refuses a non-public one.
    #[serde(default)]
    pub upstream_names_only: bool,
    /// The resolver of the egress and of a remote mint (T-076).
    #[serde(default)]
    pub dns: DnsConfig,
    /// The one repository (`owner/repo`) that the host serves; an offer of
    /// another repository is `repo-scope-refused`. `trusted` at
    /// `redirect` needs it (DR-4).
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub source: Vec<SourceConfig>,
    #[serde(default)]
    pub unit: BTreeMap<String, UnitConfig>,
    #[serde(default)]
    pub tier: BTreeMap<String, TierFloor>,
    #[serde(default)]
    pub trust: TierConfig,
    #[serde(default)]
    pub deadlines: Deadlines,
    pub pin: Pins,
    /// The unit kinds whose claim of `guest` slots may accept, after the
    /// guest self-test (GitHub #36).
    #[serde(default = "microvm")]
    pub guest_kinds: Vec<String>,
    #[serde(default)]
    pub egress: EgressConfig,
    /// The most units of the host across its kinds (T-505); none: the
    /// `slots` of each kind only.
    #[serde(default)]
    pub max_units: Option<u32>,
    /// The cache of `serve` (T-480); none: no job gets a cache.
    #[serde(default)]
    pub cache: Option<crate::cache::CacheConfig>,
    /// The memory that warm units leave free (T-487): a size, default `1G`.
    #[serde(default)]
    pub warm_reserve: Option<String>,
}

pub use crate::driver::check_driver;

pub use crate::floors::{default_floor, lowest_floor};

pub(crate) fn tier_name(t: Tier) -> &'static str {
    match t {
        Tier::Untrusted => "untrusted",
        Tier::Trusted => "trusted",
        Tier::Local => "local",
    }
}

impl Config {
    /// Read and check a file.
    pub fn parse(text: &str) -> Result<Config, String> {
        let c: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        c.check()?;
        Ok(c)
    }

    fn check(&self) -> Result<(), String> {
        self.check_trust()?;
        self.egress.check()?;
        for (name, floor) in &self.tier {
            let t: Tier = serde_json::from_value(Value::String(name.clone()))
                .map_err(|_| format!("[tier.{name}]: no tier of this name"))?;
            if floor.min_boundary < lowest_floor(t) {
                return Err(format!(
                    "[tier.{name}]: min_boundary {:?} is below the floor {:?}",
                    floor.min_boundary,
                    lowest_floor(t)
                ));
            }
        }
        // A typo would drop the guest claim with no line that names it (#283).
        // The default name stays valid with no microvm kind.
        if let Some(k) = self.guest_kinds.iter().find(|k| *k != "microvm" && !self.unit.contains_key(k.as_str())) {
            return Err(format!("guest_kinds: {k:?} is not a [unit.NAME] table"));
        }
        if self.upstream_names_only && self.upstream_proxy.is_none() {
            return Err("upstream_names_only needs upstream_proxy".into());
        }
        if self.floor(Tier::Trusted) < Boundary::Confined {
            self.check_redirect()?;
        }
        if let Some(r) = &self.repository
            && !r.split_once('/').is_some_and(|(o, n)| !o.is_empty() && !n.is_empty() && !n.contains('/'))
        {
            return Err(format!("repository {r:?} is not owner/repo"));
        }
        let mut names = std::collections::BTreeSet::new();
        for s in &self.source {
            crate::sources::check_name(&s.name)?;
            if !names.insert(s.name.as_str()) {
                return Err(format!("[[source]] {:?} is there two times", s.name));
            }
            if s.tier == Tier::Local && s.kind != SourceKind::Fake {
                return Err(format!("[[source]] {:?}: a forge source is never local (call C14)", s.name));
            }
            if s.kind == SourceKind::Fake && !self.allow_fake {
                return Err(format!("[[source]] {:?} is fake: set allow_fake = true", s.name));
            }
            if s.kind == SourceKind::GithubScaleset {
                crate::sources::check_scaleset(s)?;
            }
            crate::sources::check_personal(s)?;
            if s.kind == SourceKind::Remote {
                let missing = [
                    ("mint", &s.mint),
                    ("mint_pin", &s.mint_pin),
                    ("cert_file", &s.cert_file),
                    ("key_file", &s.key_file),
                ];
                if let Some((key, _)) = missing.iter().find(|(_, v)| v.is_none()) {
                    return Err(format!("[[source]] {:?}: a remote source needs {key}", s.name));
                }
            }
            if s.kind == SourceKind::Program && s.program.is_none() {
                return Err(format!("[[source]] {:?}: a program source needs program", s.name));
            }
            if s.kind != SourceKind::Program && (s.program.is_some() || !s.program_args.is_empty()) {
                return Err(format!("[[source]] {:?}: program and program_args are for a program source", s.name));
            }
            if !matches!(s.arch.as_str(), "x86_64" | "aarch64") {
                return Err(format!("[[source]] {:?}: arch is not x86_64 or aarch64", s.name));
            }
        }
        // The key of a host is its token at a mint. A key of two tiers lets
        // the host of the lower tier reach the mint of the higher one
        // (design source test 6).
        let remote: Vec<&SourceConfig> = self.source.iter().filter(|s| s.kind == SourceKind::Remote).collect();
        for (i, a) in remote.iter().enumerate() {
            if let Some(b) = remote[i + 1..].iter().find(|b| b.tier != a.tier && b.key_file == a.key_file) {
                return Err(format!("[[source]] {:?} and {:?}: one key_file for two tiers", a.name, b.name));
            }
        }
        if self.max_units == Some(0) {
            return Err("max_units = 0: no unit could run; remove the key or set 1 or more".into());
        }
        if self.unit.contains_key("fake") && !self.allow_fake {
            return Err("[unit.fake] needs allow_fake = true".into());
        }
        if let Some((n, _)) = self.unit.iter().find(|(n, u)| u.claim.is_some() && n.as_str() != "fake") {
            return Err(format!("[unit.{n}] claim: only the fake kind takes a claim"));
        }
        if let Some(c) = self.unit.get("fake").and_then(|u| u.claim)
            && !matches!(c, podrunner_schema::Boundary::Confined | podrunner_schema::Boundary::Guest)
        {
            return Err(format!("[unit.fake] claim {c:?}: confined or guest"));
        }
        crate::pins::check(&self.pin)?;
        crate::pins::verify(&self.pin)?;
        self.check_init()?;
        self.check_warm()?;
        crate::unit_config::check_work_folders(&self.unit)?;
        let drivers = self.unit.iter().map(|(n, u)| (format!("[unit.{n}] binary"), &u.binary));
        let sidecars = self.source.iter().map(|s| (format!("[[source]] {:?} sidecar", s.name), &s.sidecar));
        let programs = self.source.iter().map(|s| (format!("[[source]] {:?} program", s.name), &s.program));
        for (key, path) in drivers.chain(sidecars).chain(programs) {
            let Some(path) = path else { continue };
            let p = std::path::Path::new(path);
            if !p.is_absolute() {
                return Err(format!("{key} is not an absolute path: a driver never comes from PATH"));
            }
            if let Some(dir) = &self.driver_dir
                && p.parent() != Some(std::path::Path::new(dir))
            {
                return Err(format!("{key} is not in driver_dir"));
            }
        }
        Ok(())
    }

    /// The boundary floor of a tier: the configured one, or the default.
    pub fn floor(&self, t: Tier) -> Boundary {
        self.tier.get(tier_name(t)).map(|f| f.min_boundary).unwrap_or(default_floor(t))
    }
}
