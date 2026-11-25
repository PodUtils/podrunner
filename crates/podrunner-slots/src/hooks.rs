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

/// The CA bundle of the broker of a unit, for each container of its job
/// (T-578).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerCa<'a> {
    /// The name of the bundle among the credential files of the unit.
    pub file: &'a str,
    /// The bundle (PEM).
    pub pem: &'a str,
    /// The variables that name it.
    pub vars: &'a [String],
}

/// The control sockets of the units of a host.
pub trait HookPort: Send {
    /// Serve the socket of `unit` (a unit of kind `kind`) at `socket`, before
    /// the start; give the environment that the unit needs.
    /// `guest`: where the unit sees the directory of `socket`, when the
    /// driver mounts it (`UnitDriver::socket_mount`); the environment then
    /// names the guest paths. `ca`: the CA bundle of the broker of the
    /// unit, which each container of the job takes too (T-578).
    fn open(
        &self,
        kind: &str,
        driver: Arc<dyn UnitDriver>,
        unit: &UnitId,
        socket: &Path,
        guest: Option<&str>,
        ca: Option<ContainerCa<'_>>,
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
        ca: Option<ContainerCa<'_>>,
    ) -> Result<BTreeMap<String, String>, String> {
        let k = &self.kinds[kind];
        let Some(h) = &self.hooks else { return Ok(BTreeMap::new()) };
        if k.capability.container != ContainerRoute::Hooks {
            return Ok(BTreeMap::new());
        }
        h.open(&k.name, k.driver.clone(), unit, &dir.join(crate::run::CONTROL_SOCKET), guest, ca)
    }

    pub(crate) fn close_hooks(&self, unit: &UnitId) {
        if let Some(h) = &self.hooks {
            h.close(unit);
        }
    }

    /// Read the hooks of each live unit: an audit line `hook-refuse` for
    /// each refusal, and `job-started` moves `started` to `running` once.
    pub(crate) fn hook_events(&mut self, now: u64) {
        // The job-started hook of a unit asks through its egress socket, or
        // marks the first step of its own job there.
        self.answer_questions();
        let steps = self.egress.as_ref().map(|e| e.first_steps()).unwrap_or_default();
        for unit in steps {
            self.first_step(&unit, now);
        }
        self.forward_egress();
        let live: Vec<(usize, podrunner_schema::JobId)> = self
            .jobs
            .iter()
            .filter(|(_, j)| j.unit.is_some() && matches!(j.state, State::Started | State::Running))
            .map(|(k, _)| k.clone())
            .collect();
        for key in live {
            self.read_hooks(&key, now);
        }
    }

    /// The job-started hook of `unit` ran (T-660): the job of the unit moves
    /// from `started` to `running` once. The first mark wins: the container
    /// hook, the `started` event of the source, or this one. A unit with no
    /// job in `started` (a warm unit before its claim, a job that ended)
    /// changes nothing.
    pub(crate) fn first_step(&mut self, unit: &UnitId, now: u64) {
        let key = self
            .jobs
            .iter()
            .find(|(_, j)| j.unit.as_ref() == Some(unit) && j.state == State::Started)
            .map(|(k, _)| k.clone());
        if let Some(key) = key {
            self.set(&key, State::Running, now);
        }
    }

    /// Read the hooks of the unit of `key`: also at its end, before they
    /// close, so that an event that the last tick did not read is not lost
    /// (#256).
    pub(crate) fn read_hooks(&mut self, key: &(usize, podrunner_schema::JobId), now: u64) {
        let (Some(h), Some(u)) = (&self.hooks, self.jobs.get(key).and_then(|j| j.unit.clone())) else { return };
        let (refusals, events) = h.take(&u);
        let images = h.images(&u);
        if let Some(j) = self.jobs.get_mut(key) {
            for i in images {
                if !j.images.contains(&i) {
                    j.images.push(i);
                }
            }
        }
        for code in refusals {
            self.audit(audit::Kind::HookRefuse, key.0, &key.1, Some(code), None);
        }
        if events.contains(&HookEvent::JobStarted) && self.jobs[key].state == State::Started {
            self.set(key, State::Running, now);
        }
        // The time of job-completed goes to the audit; the state moves
        // on the exit of the unit only.
        if events.contains(&HookEvent::JobCompleted) {
            self.audit(audit::Kind::JobCompleted, key.0, &key.1, None, None);
        }
    }
}
