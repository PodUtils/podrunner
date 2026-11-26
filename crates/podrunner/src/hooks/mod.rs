//! The container hooks behind slots (GitHub #35): `podrunner-hook` gives
//! each unit with `container: hooks` a control socket, and the relay script
//! lies beside the sockets.

use std::path::Path;
use std::sync::Arc;

/// The hooks of a host, with the policy of each unit kind.
pub struct HooksOf {
    hooks: podrunner_hook::hooks::Hooks,
    policies: std::collections::BTreeMap<String, podrunner_hook::policy::Policy>,
}

impl podrunner_slots::hooks::HookPort for HooksOf {
    fn open(
        &self,
        kind: &str,
        driver: Arc<dyn podrunner_unit::UnitDriver>,
        unit: &podrunner_schema::UnitId,
        socket: &Path,
        guest: Option<&str>,
        ca: Option<podrunner_slots::hooks::ContainerCa<'_>>,
    ) -> Result<std::collections::BTreeMap<String, String>, String> {
        let policy = self.policies.get(kind).cloned().ok_or("no hook policy of this kind")?;
        let dir = socket.parent().ok_or("a socket with no directory")?;
        let relay = dir.join("podrunner-hook.sh");
        let io = |what: &str, e: std::io::Error| format!("{what}: {}", e);
        std::fs::create_dir_all(dir).map_err(|e| io("the socket directory", e))?;
        // A fork of another thread must not hold the relay open for write
        // when the runner starts it (T-646).
        podrunner_unit::program_file::write(&relay, podrunner_hook::RELAY).map_err(|e| io("the relay", e))?;
        // The output of the steps and the CA bundle of the containers stay
        // out of the directory that the unit sees.
        let logs = dir.with_extension("logs");
        let ca = ca.map(|c| podrunner_hook::hooks::Bundle { file: c.file, pem: c.pem, vars: c.vars, store: c.store });
        self.hooks.open(driver, unit, policy, socket, &logs, ca).map_err(|e| io("the control socket", e))?;
        // A unit that sees the directory at `guest` gets the guest paths.
        let seen = |p: &Path| match (guest, p.file_name()) {
            (Some(g), Some(name)) => format!("{}/{}", g.trim_end_matches('/'), name.to_string_lossy()),
            _ => p.display().to_string(),
        };
        Ok(std::collections::BTreeMap::from([
            ("ACTIONS_RUNNER_CONTAINER_HOOKS".into(), seen(&relay)),
            ("PODRUNNER_CTL".into(), seen(socket)),
        ]))
    }

    fn close(&self, unit: &podrunner_schema::UnitId) {
        self.hooks.close(unit);
    }

    fn cache(&self, unit: &podrunner_schema::UnitId, c: podrunner_slots::hooks::ContainerCache<'_>) {
        let cache = podrunner_hook::unit::UnitCache {
            dir: c.dir.to_path_buf(),
            target: c.target.into(),
            read_only: c.read_only,
            env: c.env.clone(),
        };
        self.hooks.set_cache(unit, Some(cache));
    }

    fn take(
        &self,
        unit: &podrunner_schema::UnitId,
    ) -> (Vec<podrunner_schema::RefusalCode>, Vec<podrunner_slots::hooks::HookEvent>) {
        use podrunner_hook::server::JobEvent;
        use podrunner_slots::hooks::HookEvent;
        let (refusals, events) = self.hooks.take(unit);
        let events = events
            .into_iter()
            .map(|e| match e {
                JobEvent::JobStarted => HookEvent::JobStarted,
                JobEvent::JobCompleted => HookEvent::JobCompleted,
            })
            .collect();
        (refusals, events)
    }

    fn images(&self, unit: &podrunner_schema::UnitId) -> Vec<String> {
        self.hooks.take_images(unit)
    }
}

impl HooksOf {
    pub fn new(policies: std::collections::BTreeMap<String, podrunner_hook::policy::Policy>) -> HooksOf {
        HooksOf { hooks: podrunner_hook::hooks::Hooks::default(), policies }
    }
}

#[cfg(test)]
mod tests;
