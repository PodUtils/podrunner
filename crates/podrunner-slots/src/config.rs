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
}

/// The slots of one unit kind.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitConfig {
    pub slots: u32,
    /// The top process of a unit: the runner in the payload.
    #[serde(default = "run_sh")]
    pub command: Vec<String>,
    /// The tool of the kind (`podvm`: the path of the podvm binary).
    #[serde(default)]
    pub binary: Option<String>,
    /// The store of the pinned images (`podvm`: a podvm store).
    #[serde(default)]
    pub images: Option<String>,
    /// The registry hosts of the container images of a job (`ghcr.io`),
    /// for `container: hooks`. None: each container is refused.
    #[serde(default)]
    pub registries: Vec<String>,
    /// The runner directory in a unit, as the runner names it; absent: the
    /// path that the driver observes (the fake unit).
    #[serde(default)]
    pub runner_dir: Option<String>,
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
    /// The egress policy ceiling of each tier (T-024).
    #[serde(default)]
    pub egress: Map<String, Value>,
}

/// Check a driver file at its use: the file and its directory belong to
/// root or to this uid, and the group and the others cannot write them.
pub fn check_driver(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: getuid has no failure and no side effect.
    let me = unsafe { libc::getuid() };
    for p in [Some(path), path.parent()].into_iter().flatten() {
        let m = std::fs::metadata(p).map_err(|e| format!("{}: {}", p.display(), e.kind()))?;
        if m.uid() != 0 && m.uid() != me {
            return Err(format!("{} belongs to another user", p.display()));
        }
        if m.mode() & 0o022 != 0 {
            return Err(format!("{} is writable by the group or the others", p.display()));
        }
    }
    Ok(())
}

/// The lowest floor of each tier that the configuration may set. `trusted`
/// may go to `redirect` with the mitigations of DR-4.
pub fn lowest_floor(t: Tier) -> Boundary {
    match t {
        Tier::Untrusted => Boundary::Guest,
        Tier::Trusted => Boundary::Redirect,
        Tier::Local => Boundary::None,
    }
}

/// The floor of a tier that the configuration does not set: `redirect`
/// for `trusted` is a choice of the operator, never a default.
pub fn default_floor(t: Tier) -> Boundary {
    match t {
        Tier::Trusted => Boundary::Confined,
        t => lowest_floor(t),
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
                let missing = [
                    ("sidecar", s.sidecar.is_none()),
                    ("config_url", s.config_url.is_none()),
                    ("installation_id", s.installation_id.is_none()),
                    ("app_id", s.app_id.is_none()),
                    ("scale_set", s.scale_set.is_none()),
                    ("key_file", s.key_file.is_none()),
                ];
                if let Some((key, _)) = missing.iter().find(|(_, m)| *m) {
                    return Err(format!("[[source]] {:?}: a github-scaleset source needs {key}", s.name));
                }
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
        crate::pins::verify(&self.pin)?;
        let drivers = self.unit.iter().map(|(n, u)| (format!("[unit.{n}] binary"), &u.binary));
        let sidecars = self.source.iter().map(|s| (format!("[[source]] {:?} sidecar", s.name), &s.sidecar));
        for (key, path) in drivers.chain(sidecars) {
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

    /// The mitigations of DR-4 that the file must state when `trusted`
    /// runs at `redirect` (GitHub #4, T-045).
    fn check_redirect(&self) -> Result<(), String> {
        if self.same_principal_ok {
            return Err(
                "[tier.trusted] redirect needs one job at a time on a shared uid: same_principal_ok = false (DR-4)"
                    .into(),
            );
        }
        if self.pin.signer.is_none() {
            return Err("[tier.trusted] redirect needs a signed pin list: [pin] signer and signature (DR-4)".into());
        }
        if self.repository.is_none() {
            return Err(
                "[tier.trusted] redirect needs one repository for the host: repository = \"owner/repo\" (DR-4)".into(),
            );
        }
        if let Some(s) = self.source.iter().find(|s| s.tier == Tier::Trusted && s.slots.is_none()) {
            return Err(format!("[[source]] {:?}: a trusted source at redirect needs slots (DR-4)", s.name));
        }
        Ok(())
    }

    /// The boundary floor of a tier: the configured one, or the default.
    pub fn floor(&self, t: Tier) -> Boundary {
        self.tier.get(tier_name(t)).map(|f| f.min_boundary).unwrap_or(default_floor(t))
    }
}
