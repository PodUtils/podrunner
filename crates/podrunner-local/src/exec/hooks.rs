//! The container hooks of a unit of the local mode (T-093).

use std::path::PathBuf;

use podrunner_broker_fake::Job;

use super::InUnit;
use super::images::registries;

/// The container hooks of one unit (T-093): the relay and the control
/// socket in its socket directory, served by a thread until [`Hooks::stop`].
pub(super) struct Hooks {
    dir: PathBuf,
    pub(super) env: std::collections::BTreeMap<String, String>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    unit: std::sync::Arc<std::sync::Mutex<podrunner_hook::unit::HookUnit>>,
    thread: std::thread::JoinHandle<()>,
}

impl Hooks {
    /// End the server; the refusals of the hook calls.
    pub(super) fn stop(self) -> Vec<String> {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        // The server checks its flag at each connection: wake it.
        let _ = std::os::unix::net::UnixStream::connect(self.dir.join("ctl.sock"));
        let _ = self.thread.join();
        let _ = std::fs::remove_dir_all(logs_dir(&self.dir));
        self.unit
            .lock()
            .map(|u| {
                let codes = u.refusals.iter().map(|r| format!("{r:?}"));
                // The cause of each refusal of the driver, for the log of the run.
                codes.chain(u.driver_errors.iter().map(|e| format!("the driver: {e}"))).collect()
            })
            .unwrap_or_default()
    }
}

impl InUnit {
    pub(super) fn hooks(
        &self,
        dir: &std::path::Path,
        message: &Job,
        id: &podrunner_schema::UnitId,
    ) -> Result<Hooks, String> {
        use podrunner_hook::server::{Unit, bind, serve_until};
        use std::os::unix::fs::PermissionsExt;
        // The socket volume alone is not enough: podvm on the namespace
        // rung has it, and no container step (T-564, T-532).
        let routed = self.driver.probe().is_ok_and(|c| c.container == podrunner_schema::ContainerRoute::Hooks);
        let Some(guest) = self.driver.socket_mount().filter(|_| routed).map(str::to_string) else {
            return Err("the job needs containers, and this unit kind has no container hooks here; run it with --microvm and --engine".into());
        };
        let dir = dir.to_path_buf();
        // The logs of the steps stay out of the socket directory, which the
        // unit writes: a link there would make the host write a file of the
        // user (T-404), as `serve` keeps them.
        let logs = logs_dir(&dir);
        // A name that another user made first is refused (T-407).
        crate::work::private_dir(&logs)?;
        let relay = dir.join("podrunner-hook.sh");
        std::fs::write(&relay, podrunner_hook::RELAY).map_err(|e| format!("the relay: {e}"))?;
        std::fs::set_permissions(&relay, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("the relay: {e}"))?;
        let unit = std::sync::Arc::new(std::sync::Mutex::new(podrunner_hook::unit::HookUnit::new(
            self.driver.clone(),
            id.clone(),
            policy(message),
            None,
            logs,
        )));
        let listener = bind(&dir.join("ctl.sock")).map_err(|e| format!("the control socket: {e}"))?;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (u, s): (std::sync::Arc<std::sync::Mutex<dyn Unit>>, _) = (unit.clone(), stop.clone());
        let thread = std::thread::spawn(move || serve_until(listener, u, s));
        let env = std::collections::BTreeMap::from([
            ("ACTIONS_RUNNER_CONTAINER_HOOKS".to_string(), format!("{guest}/podrunner-hook.sh")),
            ("PODRUNNER_CTL".to_string(), format!("{guest}/ctl.sock")),
        ]);
        Ok(Hooks { dir, env, stop, unit, thread })
    }
}

/// The hook policy of a unit of the local mode for the job `message`: the
/// registries of its images, and the hosted work folder of the runner
/// (T-590).
fn policy(message: &Job) -> podrunner_hook::policy::Policy {
    podrunner_hook::policy::Policy {
        container: podrunner_schema::ContainerRoute::Hooks,
        shared_host_net: true,
        registries: registries(message),
        work_root: "/home/runner".into(),
        work_dir: super::workdir::WORK_DIR.into(),
    }
}

/// The directory of the logs of the steps of a unit: beside its socket
/// directory, never in it (T-404).
fn logs_dir(sockets: &std::path::Path) -> PathBuf {
    sockets.with_extension("logs")
}

#[cfg(test)]
mod tests {
    /// T-590: a job container of the local mode mounts the hosted work
    /// folder; `_work` of the fake broker is refused.
    #[test]
    fn the_hook_policy_mounts_the_hosted_work_folder() {
        use podrunner_hook::input::{Command, parse};
        use podrunner_hook::policy::Gate;
        let image = format!("alpine@sha256:{}", "a".repeat(64));
        let mut job = podrunner_broker_fake::Job::scripts("a", &["true"]);
        job.overrides.insert("jobContainer".into(), serde_json::json!(image));
        let prepare = |source: &str| {
            let mount =
                serde_json::json!({ "sourceVolumePath": source, "targetVolumePath": "/__w", "readOnly": false });
            let c = serde_json::json!({ "image": image, "systemMountVolumes": [mount] });
            let body = serde_json::json!({ "command": "prepare_job", "args": { "container": c, "services": [] } });
            Gate::default()
                .check(&parse(Command::PrepareJob, body.to_string().as_bytes()).unwrap(), &super::policy(&job))
        };
        assert!(prepare("/home/runner/work").is_ok());
        let e = prepare("/home/runner/_work").unwrap_err();
        assert_eq!(e.code, podrunner_schema::RefusalCode::ContainerOptionRefused);
    }

    /// T-404: the logs are out of the directory that the unit writes.
    #[test]
    fn the_logs_are_out_of_the_socket_directory() {
        let sockets = std::path::Path::new("/tmp/podrunner-local-unit-1-u-1");
        let logs = super::logs_dir(sockets);
        assert!(!logs.starts_with(sockets), "{}", logs.display());
        assert_eq!(logs.parent(), sockets.parent());
    }
}
