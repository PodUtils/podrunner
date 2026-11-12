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
    ) -> Result<std::collections::BTreeMap<String, String>, String> {
        use std::os::unix::fs::PermissionsExt;
        let policy = self.policies.get(kind).cloned().ok_or("no hook policy of this kind")?;
        let dir = socket.parent().ok_or("a socket with no directory")?;
        let relay = dir.join("podrunner-hook.sh");
        let io = |what: &str, e: std::io::Error| format!("{what}: {}", e.kind());
        std::fs::create_dir_all(dir).map_err(|e| io("the socket directory", e))?;
        std::fs::write(&relay, podrunner_hook::RELAY).map_err(|e| io("the relay", e))?;
        std::fs::set_permissions(&relay, std::fs::Permissions::from_mode(0o755)).map_err(|e| io("the relay", e))?;
        // The output of the steps stays out of the directory that the unit
        // sees.
        let logs = dir.with_extension("logs");
        self.hooks.open(driver, unit, policy, socket, &logs).map_err(|e| io("the control socket", e))?;
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
