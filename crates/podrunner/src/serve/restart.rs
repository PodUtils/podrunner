//! A new key of a source with no drain (T-498, githome's G-22): `podrunner
//! source restart` builds one source again from its table, with its key
//! file of now; a source that revoked podrunner starts again when its key
//! file changes. The jobs of the source stay.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use pod_core::clock::Clock;
use podrunner_slots::Slots;
use podrunner_source::HostFacts;

/// What `serve` needs to build a source again.
pub(crate) struct Rebuild {
    pub clock: Arc<dyn Clock>,
    pub origin: Instant,
    pub lookup: Option<podrunner_remote::transport::Lookup>,
    pub facts: Option<HostFacts>,
    /// The time of change of the key file of each source, when it was
    /// built.
    seen: BTreeMap<String, Option<SystemTime>>,
}

fn changed_at(path: Option<&str>) -> Option<SystemTime> {
    std::fs::metadata(path?).and_then(|m| m.modified()).ok()
}

impl Rebuild {
    pub(crate) fn new(
        cfg: &podrunner_slots::config::Config,
        clock: Arc<dyn Clock>,
        origin: Instant,
        lookup: Option<podrunner_remote::transport::Lookup>,
    ) -> Rebuild {
        let seen = cfg.source.iter().map(|s| (s.name.clone(), changed_at(s.key_file.as_deref()))).collect();
        Rebuild { clock, origin, lookup, facts: None, seen }
    }

    /// Build the source `name` again and put it in its place. A source
    /// that does not build, or refuses `hello`, leaves the old one.
    pub(crate) fn restart(&mut self, slots: &mut Slots, name: &str) -> Result<(), String> {
        let cfg = slots.source_config(name).ok_or_else(|| format!("no source {name:?}"))?;
        let at = changed_at(cfg.key_file.as_deref());
        let mut source = super::source_of(&cfg, self.clock.clone(), self.origin, self.lookup.clone())
            .map_err(|f| format!("{name}: {}", f.detail))?;
        let version = env!("CARGO_PKG_VERSION");
        let hello = match &self.facts {
            Some(facts) => source.hello_host("podrunner", version, facts),
            None => source.hello("podrunner", version),
        };
        hello.map_err(|e| format!("{name}: hello: {e}"))?;
        slots.replace_source(name, source)?;
        self.seen.insert(name.to_string(), at);
        Ok(())
    }

    /// Start again each source that revoked podrunner and whose key file
    /// changed since it was built: one line for each try.
    pub(crate) fn watch(&mut self, slots: &mut Slots) -> Vec<String> {
        let mut said = Vec::new();
        for name in slots.revoked_sources() {
            let Some(cfg) = slots.source_config(&name) else { continue };
            let now = changed_at(cfg.key_file.as_deref());
            if now.is_none() || self.seen.get(&name) == Some(&now) {
                continue;
            }
            // One try for each change of the file: no loop on a bad key.
            self.seen.insert(name.clone(), now);
            said.push(match self.restart(slots, &name) {
                Ok(()) => format!("{name}: the key file changed: the source started again"),
                Err(e) => format!("the key file changed, and the source did not start: {e}"),
            });
        }
        said
    }
}
