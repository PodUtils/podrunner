//! The container hooks as slots sees them (GitHub #35): a control socket
//! for each unit whose kind has `container: hooks`, open from the start of
//! the unit to its destroy. `podrunner-hook` is behind the seam.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use podrunner_schema::{ContainerRoute, RefusalCode, UnitId};
use podrunner_unit::UnitDriver;

use crate::audit;
use crate::engine::Slots;
use crate::state::State;

/// A job hook of the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    JobStarted,
    JobCompleted,
}

/// The control sockets of the units of a host.
pub trait HookPort: Send {
    /// Serve the socket of `unit` (a unit of kind `kind`) at `socket`, before
    /// the start; give the environment that the unit needs.
    /// `guest`: where the unit sees the directory of `socket`, when the
    /// driver mounts it (`UnitDriver::socket_mount`); the environment then
    /// names the guest paths.
    fn open(
        &self,
        kind: &str,
        driver: Arc<dyn UnitDriver>,
        unit: &UnitId,
        socket: &Path,
        guest: Option<&str>,
    ) -> Result<BTreeMap<String, String>, String>;
    fn close(&self, unit: &UnitId);
    /// The refusals and the job hooks of `unit` since the last call.
    fn take(&self, unit: &UnitId) -> (Vec<RefusalCode>, Vec<HookEvent>);
    /// The digests of the images that the containers of `unit` started
    /// with since the last call.
    fn images(&self, _unit: &UnitId) -> Vec<String> {
        Vec::new()
    }
}

impl Slots {
    /// Give each unit with `container: hooks` a control socket.
    pub fn set_hooks(&mut self, hooks: Box<dyn HookPort>) {
        self.hooks = Some(hooks);
    }

    /// Open the socket of the unit of `key` when its kind has the container
    /// route; the environment of the unit, or why it failed.
    pub(crate) fn open_hooks(
        &self,
        kind: usize,
        unit: &UnitId,
        dir: &std::path::Path,
        guest: Option<&str>,
    ) -> Result<BTreeMap<String, String>, String> {
        let k = &self.kinds[kind];
        let Some(h) = &self.hooks else { return Ok(BTreeMap::new()) };
        if k.capability.container != ContainerRoute::Hooks {
            return Ok(BTreeMap::new());
        }
        h.open(&k.name, k.driver.clone(), unit, &dir.join(crate::run::CONTROL_SOCKET), guest)
    }

    pub(crate) fn close_hooks(&self, unit: &UnitId) {
        if let Some(h) = &self.hooks {
            h.close(unit);
        }
    }

    /// Read the hooks of each live unit: an audit line `hook-refuse` for
    /// each refusal, and `job-started` moves `started` to `running` once.
    pub(crate) fn hook_events(&mut self, now: u64) {
        let Some(h) = &self.hooks else { return };
        let mut seen = Vec::new();
        for (key, j) in &self.jobs {
            if let (Some(u), State::Started | State::Running) = (&j.unit, j.state) {
                let (refusals, events) = h.take(u);
                let images = h.images(u);
                if !refusals.is_empty() || !events.is_empty() || !images.is_empty() {
                    seen.push((key.clone(), refusals, events, images));
                }
            }
        }
        for (key, refusals, events, images) in seen {
            let j = self.jobs.get_mut(&key).expect("a live job");
            for i in images {
                if !j.images.contains(&i) {
                    j.images.push(i);
                }
            }
            for code in refusals {
                self.audit(audit::Kind::HookRefuse, key.0, &key.1, Some(code), None);
            }
            if events.contains(&HookEvent::JobStarted) && self.jobs[&key].state == State::Started {
                self.set(&key, State::Running, now);
            }
            // The time of job-completed goes to the audit; the state moves
            // on the exit of the unit only.
            if events.contains(&HookEvent::JobCompleted) {
                self.audit(audit::Kind::JobCompleted, key.0, &key.1, None, None);
            }
        }
    }
}
