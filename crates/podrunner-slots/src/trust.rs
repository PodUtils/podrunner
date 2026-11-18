//! The trust lists of each source (T-494, githome's G-16). A login is a
//! name in the namespace of one forge: `alice` of githome is not `alice` of
//! GitHub. A `[[source]] trust` table holds in place of `[trust]` for the
//! offers of its source.

use podrunner_tier::TierConfig;

use crate::config::{Config, SourceConfig, SourceKind};

impl Config {
    /// The trust lists that hold for the offers of `source`.
    pub fn trust_of<'a>(&'a self, source: &'a SourceConfig) -> &'a TierConfig {
        source.trust.as_ref().unwrap_or(&self.trust)
    }

    /// Each list is valid, and no `program` source shares the global list
    /// with another source: a `program` source can carry the jobs of a
    /// forge that is not GitHub, where the same login is another owner.
    pub(crate) fn check_trust(&self) -> Result<(), String> {
        self.trust.check()?;
        for s in &self.source {
            if let Some(t) = &s.trust {
                t.check().map_err(|e| format!("[[source]] {}: {e}", s.name))?;
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
