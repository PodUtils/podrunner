//! The facts of the kinds of this host for its sources (T-495), and the
//! `hello` that gives them.

use podrunner_source::{HostFacts, KindFacts};

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
