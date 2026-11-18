//! The facts of the kinds of this host for its sources (T-495), and the
//! `hello` that gives them.

use podrunner_source::{HostFacts, JobSource, KindFacts};

use crate::config::SourceConfig;

use crate::engine::{Job, Slots};
use crate::placement::Kind;

impl Slots {
    pub(crate) fn kind_views(&self) -> Vec<Kind> {
        // The cap of the host across its kinds (T-505).
        let running = self.jobs.values().filter(|j| j.unit.is_some() && !j.state.is_final()).count() as u32;
        let host_free = self.cfg.max_units.map_or(u32::MAX, |m| m.saturating_sub(running));
        self.kinds
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let units: Vec<&Job> = self
                    .jobs
                    .values()
                    .filter(|j| {
                        j.unit.is_some() && !j.state.is_final() && j.placement.as_ref().is_some_and(|p| p.kind == i)
                    })
                    .collect();
                let repos = units
                    .iter()
                    .filter_map(|j| {
                        j.offer.as_ref().map(|o| format!("{}/{}", o.trust_inputs.owner, o.trust_inputs.repo))
                    })
                    .collect();
                Kind {
                    capability: k.capability.clone(),
                    free: k.cfg.slots.saturating_sub(units.len() as u32).min(host_free),
                    running: units.len() as u32,
                    repos,
                }
            })
            .collect()
    }

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

    /// Drain the source `name` (T-501): it takes no offer, and its jobs in
    /// flight go on; `on = false` takes offers again. Each other source goes
    /// on.
    pub fn drain_source(&mut self, name: &str, on: bool) -> Result<(), String> {
        if !self.sources.iter().any(|s| s.cfg.name == name) {
            return Err(format!("no source {name:?}"));
        }
        if on {
            self.drained.insert(name.to_string());
        } else {
            self.drained.remove(name);
            self.stopped.remove(name);
        }
        Ok(())
    }

    /// Stop the source `name` (T-501): drain it, and when its last job
    /// ends, [`Slots::end_stopped`] puts `sources::Stopped` in its
    /// place, so that its program ends.
    pub fn stop_source(&mut self, name: &str) -> Result<(), String> {
        self.drain_source(name, true)?;
        self.stopped.entry(name.to_string()).or_insert(false);
        Ok(())
    }

    /// True when the source `name` is stopped and its program ended.
    pub fn source_ended(&self, name: &str) -> bool {
        self.stopped.get(name) == Some(&true)
    }

    /// End each stopped source with no job: the names of the ones that
    /// ended now.
    pub fn end_stopped(&mut self) -> Vec<String> {
        let mut ended = Vec::new();
        for (i, s) in self.sources.iter_mut().enumerate() {
            let busy = self.jobs.keys().any(|(j, _)| *j == i);
            if let Some(done @ false) = self.stopped.get_mut(&s.cfg.name)
                && !busy
            {
                s.source = Box::new(crate::sources::Stopped);
                *done = true;
                ended.push(s.cfg.name.clone());
            }
        }
        ended
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
