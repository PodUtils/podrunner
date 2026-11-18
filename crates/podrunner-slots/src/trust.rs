//! The trust lists of each source (T-494, githome's G-16). A login is a
//! name in the namespace of one forge: `alice` of githome is not `alice` of
//! GitHub. A `[[source]] trust` table holds in place of `[trust]` for the
//! offers of its source; a list of tables, each with its `forge`, holds for
//! the offers whose `trust_inputs.forge` names that forge, so that one
//! source can carry the jobs of two forges.

use podrunner_schema::RefusalCode;
use podrunner_schema::offer::TrustInputs;
use podrunner_tier::{Organization, PublicRepo, TierConfig};
use serde::Deserialize;

use crate::config::{Config, SourceConfig, SourceKind};

/// The trust lists of one source.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum SourceTrust {
    /// One table for each offer of the source.
    One(TierConfig),
    /// One table for each forge.
    ByForge(Vec<ForgeTrust>),
}

/// The lists of one forge of a source.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "RawForge")]
pub struct ForgeTrust {
    /// The host name of the forge, in lower case (`github.com`).
    pub forge: String,
    pub lists: TierConfig,
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawForge {
    forge: String,
    #[serde(default)]
    organizations: Vec<Organization>,
    #[serde(default)]
    users: Vec<Organization>,
    #[serde(default)]
    trusted_public: Vec<PublicRepo>,
    #[serde(default)]
    extra_events: Vec<podrunner_schema::offer::Event>,
}

impl From<RawForge> for ForgeTrust {
    fn from(r: RawForge) -> ForgeTrust {
        ForgeTrust {
            forge: r.forge.to_ascii_lowercase(),
            lists: TierConfig {
                organizations: r.organizations,
                users: r.users,
                trusted_public: r.trusted_public,
                extra_events: r.extra_events,
            },
        }
    }
}

impl Config {
    /// The trust lists that hold for an offer of `source` with the facts
    /// `t`; an offer with no forge, or with a forge that the lists of the
    /// source do not name, is refused (`tier-mismatch`).
    pub fn trust_for<'a>(&'a self, source: &'a SourceConfig, t: &TrustInputs) -> Result<&'a TierConfig, RefusalCode> {
        match &source.trust {
            None => Ok(&self.trust),
            Some(SourceTrust::One(lists)) => Ok(lists),
            Some(SourceTrust::ByForge(forges)) => {
                let forge = t.forge.as_deref().map(str::to_ascii_lowercase);
                forges
                    .iter()
                    .find(|f| Some(&f.forge) == forge.as_ref())
                    .map(|f| &f.lists)
                    .ok_or(RefusalCode::TierMismatch)
            }
        }
    }

    /// Each list is valid, each forge of a source is named once, and no
    /// `program` source shares the global list with another source: a
    /// `program` source can carry the jobs of a forge that is not GitHub,
    /// where the same login is another owner.
    pub(crate) fn check_trust(&self) -> Result<(), String> {
        self.trust.check()?;
        for s in &self.source {
            let named = |e: String| format!("[[source]] {}: {e}", s.name);
            match &s.trust {
                None => {}
                Some(SourceTrust::One(t)) => t.check().map_err(named)?,
                Some(SourceTrust::ByForge(forges)) => {
                    for (i, f) in forges.iter().enumerate() {
                        if f.forge.is_empty() || forges[..i].iter().any(|g| g.forge == f.forge) {
                            return Err(named(format!("trust: the forge {:?} is empty or named twice", f.forge)));
                        }
                        f.lists.check().map_err(named)?;
                    }
                }
            }
        }
        let t = &self.trust;
        let global = !(t.organizations.is_empty() && t.users.is_empty() && t.trusted_public.is_empty());
        let sharing: Vec<&SourceConfig> =
            self.source.iter().filter(|s| s.trust.is_none() && s.kind != SourceKind::Fake).collect();
        if global && sharing.len() > 1 && sharing.iter().any(|s| s.kind == SourceKind::Program) {
            let names: Vec<&str> = sharing.iter().map(|s| s.name.as_str()).collect();
            return Err(format!(
                "[trust] holds for the sources {}: a login of one forge can match a login of another; give each \
                 source its own trust table ([[source]] trust = {{ users = [...], organizations = [...] }})",
                names.join(", ")
            ));
        }
        Ok(())
    }
}
