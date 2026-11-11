//! `podrunner.toml` (`docs/configuration.md`). A file that lowers the
//! floor of a tier, or that names a `fake` part with no `allow_fake`, is
//! refused. An unknown key is refused.

use std::collections::BTreeMap;

use podrunner_schema::{Boundary, Tier};
use podrunner_tier::TierConfig;
use serde::Deserialize;
use serde_json::{Map, Value};

/// The kind of a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    Fake,
    GithubScaleset,
}

/// One source instance: one set of one tier, one architecture, one host.
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
}

/// The slots of one unit kind.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitConfig {
    pub slots: u32,
    /// The top process of a unit: the runner in the payload.
    #[serde(default = "run_sh")]
    pub command: Vec<String>,
}

fn run_sh() -> Vec<String> {
    vec!["./run.sh".into()]
}

/// The floor of one tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierFloor {
    pub min_boundary: Boundary,
}

/// The time limit of each state, in seconds.
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub arch: String,
    /// `NAME@sha256:HEX`.
    pub reference: String,
    /// The time of the build, in seconds of the Unix epoch.
    pub built_at_s: u64,
}

/// The pin list.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pins {
    /// An image older than this is `image-stale`.
    #[serde(default = "thirty_days")]
    pub max_age_s: u64,
    #[serde(default)]
    pub payload: Vec<Payload>,
}

fn thirty_days() -> u64 {
    30 * 24 * 3600
}

fn hour() -> u64 {
    3600
}

/// The whole file.
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
    /// The egress policy ceiling of each tier (T-024).
    #[serde(default)]
    pub egress: Map<String, Value>,
}

/// The lowest floor of each tier that the configuration may set. A ruling
/// of the owner changes the floor of `trusted` (DR-4, Q1).
pub fn lowest_floor(t: Tier) -> Boundary {
    match t {
        Tier::Untrusted => Boundary::Guest,
        Tier::Trusted => Boundary::Confined,
        Tier::Local => Boundary::None,
    }
}

fn tier_name(t: Tier) -> &'static str {
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
        let mut names = std::collections::BTreeSet::new();
        for s in &self.source {
            if !names.insert(s.name.as_str()) {
                return Err(format!("[[source]] {:?} is there two times", s.name));
            }
            if s.tier == Tier::Local && s.kind != SourceKind::Fake {
                return Err(format!("[[source]] {:?}: a forge source is never local (call C14)", s.name));
            }
            if s.kind == SourceKind::Fake && !self.allow_fake {
                return Err(format!("[[source]] {:?} is fake: set allow_fake = true", s.name));
            }
            if !matches!(s.arch.as_str(), "x86_64" | "aarch64") {
                return Err(format!("[[source]] {:?}: arch is not x86_64 or aarch64", s.name));
            }
        }
        if self.unit.contains_key("fake") && !self.allow_fake {
            return Err("[unit.fake] needs allow_fake = true".into());
        }
        for p in &self.pin.payload {
            podrunner_unit::parse_pinned(&p.reference).map_err(|e| format!("[[pin.payload]]: {e}"))?;
        }
        Ok(())
    }

    /// The boundary floor of a tier: the configured one, or the lowest.
    pub fn floor(&self, t: Tier) -> Boundary {
        self.tier.get(tier_name(t)).map(|f| f.min_boundary).unwrap_or(lowest_floor(t))
    }
}
