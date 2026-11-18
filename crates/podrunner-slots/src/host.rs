//! The facts of the kinds of this host for its sources (T-495), and the
//! `hello` that gives them.

use podrunner_source::{HostFacts, JobSource, KindFacts};

use crate::config::SourceConfig;

use crate::engine::Slots;

impl Slots {
    /// The facts of each unit kind: its boundary, its container route, and
    /// the largest CPU, memory, disk and processes that a unit gets.
    pub fn kind_facts(&self) -> Vec<KindFacts> {
        self.kinds
            .iter()
            .map(|k| {
                let lim = crate::limits::limits_of(&k.cfg).unwrap_or_default();
                KindFacts {
                    name: k.name.clone(),
                    boundary: k.capability.boundary,
                    container: k.capability.container,
                    cpu: lim.cpus.map(|c| u64::from(c) * 1000),
                    memory: lim.memory_bytes,
                    disk: k.cfg.disk_max_bytes,
                    pids: lim.pids.map(u64::from),
                }
            })
            .collect()
    }

    /// The configuration of the source `name`.
    pub fn source_config(&self, name: &str) -> Option<SourceConfig> {
        self.sources.iter().find(|s| s.cfg.name == name).map(|s| s.cfg.clone())
    }

    /// The names of the sources that revoked podrunner.
    pub fn revoked_sources(&self) -> Vec<String> {
        self.sources.iter().filter(|s| s.revoked).map(|s| s.cfg.name.clone()).collect()
    }

    /// Put `source` in the place of the source `name` (T-498): a new key
    /// with no drain. Each job of the source stays; its next call goes to
    /// the new one, and the source gets capacity again.
    pub fn replace_source(&mut self, name: &str, source: Box<dyn JobSource>) -> Result<(), String> {
        let s = self.sources.iter_mut().find(|s| s.cfg.name == name).ok_or_else(|| format!("no source {name:?}"))?;
        s.source = source;
        s.revoked = false;
        Ok(())
    }

    /// Send `hello` with `host` to each source. One line for each source
    /// that refused it; a source with no facts in its wire refuses the
    /// field, and goes on as before.
    pub fn hello_sources(&mut self, client: &str, version: &str, host: &HostFacts) -> Vec<String> {
        let mut refused = Vec::new();
        for s in &mut self.sources {
            if let Err(e) = s.source.hello_host(client, version, host) {
                refused.push(format!("{}: {e}", s.cfg.name));
            }
        }
        refused
    }
}
