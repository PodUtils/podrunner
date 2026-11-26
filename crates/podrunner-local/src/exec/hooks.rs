//! The container hooks of a unit of the local mode (T-093).

use std::path::PathBuf;

use podrunner_broker_fake::Job;

use super::InUnit;
use super::images::registries;

/// The longest wait for the hook server at the end of a job (short in a
/// test, which holds a hook call on purpose).
const STOP_WAIT: std::time::Duration = std::time::Duration::from_secs(if cfg!(test) { 1 } else { 10 });

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
    /// End the server; one line of the log for each refusal of a hook call
    /// and for each cause of the driver. A hook call that still runs after
    /// [`STOP_WAIT`] stays behind: the run does not wait for it (T-639).
    pub(super) fn stop(self) -> Vec<String> {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        // The server checks its flag at each connection: wake it.
        let _ = std::os::unix::net::UnixStream::connect(self.dir.join("ctl.sock"));
        let t = std::time::Instant::now();
        while !self.thread.is_finished() && t.elapsed() < STOP_WAIT {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let mut out: Vec<String> = match self.thread.is_finished() {
            true => {
                let _ = self.thread.join();
                Vec::new()
            }
            false => {
                vec![format!("a hook call did not end in {} s; the run does not wait for it", STOP_WAIT.as_secs())]
            }
        };
        let _ = std::fs::remove_dir_all(logs_dir(&self.dir));
        out.extend(lines(&self.unit));
        out
    }
}

/// The refusals and the driver errors of the hook calls of a unit, as lines
/// of the log. A call that still runs holds the unit: no wait for it, and
/// one line says that the log has no refusal of it.
fn lines(unit: &std::sync::Mutex<podrunner_hook::unit::HookUnit>) -> Vec<String> {
    use std::sync::TryLockError;
    let u = match unit.try_lock() {
        Ok(u) => u,
        // A hook call that panicked: its record is still true.
        Err(TryLockError::Poisoned(p)) => p.into_inner(),
        Err(TryLockError::WouldBlock) => {
            return vec![
                "a hook call still holds the unit: its refusals and its driver errors are not in this log".into(),
            ];
        }
    };
    let refused = u.refusals.iter().map(|r| format!("a hook call was refused: {r:?}"));
    // The cause of each refusal of the driver.
    refused.chain(u.driver_errors.iter().map(|e| format!("a hook call failed in the driver: {e}"))).collect()
}

impl InUnit {
    pub(super) fn hooks(
        &self,
        dir: &std::path::Path,
        message: &Job,
        id: &podrunner_schema::UnitId,
        ca: Option<&str>,
    ) -> Result<Hooks, String> {
        use podrunner_hook::server::{Unit, bind, serve_until};
        // The socket volume alone is not enough: podvm on the namespace
        // rung has it, and no container step (T-564, T-532).
        let routed = self.driver.probe().is_ok_and(|c| c.container == podrunner_schema::ContainerRoute::Hooks);
        let Some(guest) = self.driver.socket_mount().filter(|_| routed).map(str::to_string) else {
            return Err("the job needs containers, and this unit kind has no container hooks here; run it with --microvm and --engine".into());
        };
        let dir = dir.to_path_buf();
        let relay = dir.join("podrunner-hook.sh");
        // A fork of another thread must not hold the relay open for write
        // when the runner starts it (T-646).
        podrunner_unit::program_file::write(&relay, podrunner_hook::RELAY).map_err(|e| format!("the relay: {e}"))?;
        // The logs of the steps stay out of the socket directory, which the
        // unit writes: a link there would make the host write a file of the
        // user (T-404), as `serve` keeps them.
        let logs = logs_dir(&dir);
        // A name that another user made first is refused (T-407).
        crate::work::private_dir(&logs)?;
        // Each container trusts the CA files of the caller, as the runner
        // does: the copy is out of the socket directory, as `serve` keeps
        // it (T-596).
        let vars = super::proxy::container_vars();
        let bundle = |pem| podrunner_hook::hooks::Bundle { file: super::proxy::CONTAINER_CA, pem, vars: &vars };
        let opened = || -> Result<_, String> {
            let ca = ca.map(|pem| podrunner_hook::hooks::place_ca(&logs, bundle(pem))).transpose();
            let ca = ca.map_err(|e| format!("the CA bundle of the containers: {e}"))?;
            Ok((ca, bind(&dir.join("ctl.sock")).map_err(|e| format!("the control socket: {e}"))?))
        };
        let (ca, listener) = opened().inspect_err(|_| {
            // No socket serves the unit: no container takes the copy.
            let _ = std::fs::remove_dir_all(&logs);
        })?;
        let hook = podrunner_hook::unit::HookUnit::new(self.driver.clone(), id.clone(), policy(message), None, logs);
        let unit = std::sync::Arc::new(std::sync::Mutex::new(hook.with_ca(ca)));
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
    /// T-639: a unit that a hook call holds gives one line that says so; a
    /// free unit gives one line for each refusal and each driver error.
    #[test]
    fn the_lines_of_a_unit_that_a_hook_call_holds_say_so() {
        let dir = std::env::temp_dir().join(format!("podrunner-local-hook-lines-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let driver =
            std::sync::Arc::new(podrunner_fake::FakeUnit::new(&dir, super::super::tests::capability(), "linux/amd64"));
        let id = podrunner_schema::UnitId::parse("u-1").unwrap();
        let mut hook = podrunner_hook::unit::HookUnit::new(driver, id, super::policy(&job()), None, dir.join("logs"));
        hook.refusals.push(podrunner_schema::RefusalCode::ContainerOptionRefused);
        hook.driver_errors.push("no engine".into());
        let unit = std::sync::Mutex::new(hook);
        let free = super::lines(&unit);
        assert_eq!(free.len(), 2, "{free:?}");
        assert!(free[0].starts_with("a hook call was refused: ContainerOptionRefused"), "{free:?}");
        assert_eq!(free[1], "a hook call failed in the driver: no engine");
        let held = unit.lock().unwrap();
        let busy = super::lines(&unit);
        drop(held);
        assert_eq!(busy.len(), 1, "{busy:?}");
        assert!(busy[0].starts_with("a hook call still holds the unit: "), "{busy:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn job() -> podrunner_broker_fake::Job {
        podrunner_broker_fake::Job::scripts("a", &["true"])
    }

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

    /// A made-up certificate: the containers read it, no TLS takes it.
    const ONE: &str = "-----BEGIN CERTIFICATE-----\nbWFkZS11cA==\n-----END CERTIFICATE-----\n";
    const TWO: &str = "-----BEGIN CERTIFICATE-----\nb3RoZXI=\n-----END CERTIFICATE-----";
    const IMAGE: &str = "node@sha256:0000000000000000000000000000000000000000000000000000000000000000";

    /// The hooks of a unit of `podrunner run` for a caller with the
    /// variables `vars`; a container step that prints the bundle and each
    /// CA variable; the output and the record of the container.
    fn container_step(dir: &std::path::Path, vars: &[(&str, String)], job_env: serde_json::Value) -> (String, String) {
        use podrunner_hook::input::{Command, parse};
        use podrunner_hook::server::Unit;
        use podrunner_unit::UnitDriver;
        let vars = vars.iter().map(|(k, v)| (k.to_string(), v.clone()));
        let (_, _, ca) = super::super::proxy::unit_env("/podrunner/.runner", vars, false).unwrap();
        let fake = podrunner_fake::FakeUnit::new(&dir.join("units"), super::super::tests::capability(), "linux/amd64");
        let driver = std::sync::Arc::new(super::super::tests::Mounted(fake, std::sync::Mutex::new(None)));
        let image = driver.prepare(IMAGE, "linux/amd64", false).unwrap();
        let id = podrunner_schema::UnitId::parse("u-1").unwrap();
        let start = podrunner_unit::StartSpec {
            unit: id.clone(),
            image: image.clone(),
            argv: vec!["sleep".into(), "30".into()],
            env: Default::default(),
            credential: Default::default(),
            sockets: None,
            mounts: Vec::new(),
            limits: Default::default(),
        };
        driver.0.start(&start).unwrap();
        let exec = super::InUnit::new(driver.clone(), image);
        let mut job = podrunner_broker_fake::Job::scripts("a", &["true"]);
        job.overrides.insert("jobContainer".into(), serde_json::json!(IMAGE));
        let sockets = dir.join("sockets");
        std::fs::create_dir_all(&sockets).unwrap();
        let hooks = exec.hooks(&sockets, &job, &id, ca.as_deref()).unwrap();
        let names = super::super::proxy::container_vars().join(" ");
        let script = format!(r#"cat "${{SSL_CERT_FILE:-/nothing}}" 2>/dev/null; echo; printenv {names}; s=${{JAVA_TOOL_OPTIONS#*trustStore=}}; test -s "${{s%% *}}" && echo "java: $JAVA_TOOL_OPTIONS"; true"#);
        let c = serde_json::json!({
            "image": IMAGE, "entryPoint": "sh", "entryPointArgs": ["-c", script],
            "environmentVariables": job_env, "portMappings": [], "userMountVolumes": [], "systemMountVolumes": [],
        });
        let body = serde_json::json!({ "command": "run_container_step", "responseFile": "/home/runner/_work/_temp/r.json", "args": c, "state": {} });
        let out =
            hooks.unit.lock().unwrap().hook(parse(Command::RunContainerStep, body.to_string().as_bytes()).unwrap());
        let out = String::from_utf8(out.unwrap().output).unwrap();
        let root = driver.observe(&id).unwrap().unit_root.unwrap();
        let record = std::fs::read_to_string(root.join("containers/c-0/spec")).unwrap();
        // The copy is beside the socket directory, and goes with the hooks.
        let copy = super::logs_dir(&sockets).join("ca").join(super::super::proxy::CONTAINER_CA);
        if ca.is_some() {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&copy).unwrap().permissions().mode() & 0o777, 0o644);
        }
        assert!(hooks.stop().is_empty());
        assert!(!copy.exists() && !super::logs_dir(&sockets).exists(), "the stop removes the copy");
        let _ = driver.destroy(&id);
        (out, record)
    }

    /// T-596: a container of a unit of a caller with a CA file gets one
    /// bundle of each CA file, read-only at `/__podrunner`, and the ten
    /// variables name it. A link, a file at mode 0600, a path with a space
    /// and two variables of one file give each file once; a file with no
    /// certificate stays out.
    #[test]
    fn a_container_of_a_caller_with_a_ca_file_gets_the_mount_and_the_variables() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("podrunner-local-ca-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a b")).unwrap();
        let (one, two, empty, link) =
            (dir.join("a b/one.pem"), dir.join("two.pem"), dir.join("empty.pem"), dir.join("link.pem"));
        std::fs::write(&one, ONE).unwrap();
        std::fs::set_permissions(&one, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::write(&two, TWO).unwrap();
        std::fs::write(&empty, "").unwrap();
        std::os::unix::fs::symlink(&one, &link).unwrap();
        let p = |f: &std::path::Path| f.display().to_string();
        let vars = [
            ("SSL_CERT_FILE", p(&link)),
            ("NODE_EXTRA_CA_CERTS", p(&link)),
            ("CURL_CA_BUNDLE", p(&one)),
            ("GIT_SSL_CAINFO", p(&two)),
            ("REQUESTS_CA_BUNDLE", p(&empty)),
        ];
        let (out, record) = container_step(&dir, &vars, serde_json::json!({}));
        let host = dir.join("sockets.logs/ca/podrunner-ca.pem");
        assert!(record.contains(&format!("mount {} /__podrunner ro\n", host.parent().unwrap().display())), "{record}");
        // The fake maps `/__podrunner` to its source: the step reads the bundle.
        assert!(out.starts_with(&format!("{ONE}{TWO}\n\n")), "each file once, in order: {out}");
        let lines: Vec<&str> = out.lines().filter(|l| l.starts_with('/')).collect();
        assert_eq!(lines, vec![host.to_str().unwrap(); 10], "{out}");
        // A JVM gets the trust store beside the bundle (T-616).
        let store = host.with_extension("p12").display().to_string();
        let java = format!("-Djavax.net.ssl.trustStore={store} -Djavax.net.ssl.trustStoreType=PKCS12");
        assert!(out.ends_with(&format!("\njava: {java}\n")), "{out}");
        // A value that the job sets stays; a value of `JAVA_TOOL_OPTIONS`
        // comes after the store, so that its own store has priority.
        let job = serde_json::json!({ "SSL_CERT_FILE": "/job/ca.pem", "JAVA_TOOL_OPTIONS": "-Xmx1g" });
        let (out, _) = container_step(&dir, &vars, job);
        assert!(out.contains("\n/job/ca.pem\n"), "{out}");
        assert_eq!(out.lines().filter(|l| *l == host.to_str().unwrap()).count(), 9, "{out}");
        assert!(out.ends_with(&format!("\njava: {java} -Xmx1g\n")), "{out}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-596: a caller with no CA file (or with only a file with no
    /// certificate) gives a container no mount and no variable.
    #[test]
    fn a_container_of_a_caller_with_no_ca_file_gets_neither() {
        let dir = std::env::temp_dir().join(format!("podrunner-local-noca-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("empty.pem"), "").unwrap();
        let empty = dir.join("empty.pem").display().to_string();
        let proxy = [("HTTPS_PROXY", "http://127.0.0.1:3128".to_string())];
        for vars in [&proxy[..], &[("SSL_CERT_FILE", empty)]] {
            let (out, record) = container_step(&dir, vars, serde_json::json!({}));
            assert_eq!(out, "\n", "{vars:?}");
            assert!(!record.contains("/__podrunner"), "{record}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-596: a socket that does not bind leaves no copy of the bundle.
    #[test]
    fn a_socket_that_does_not_bind_leaves_no_copy() {
        let dir = std::env::temp_dir().join(format!("podrunner-local-ca-nobind-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fake = podrunner_fake::FakeUnit::new(&dir.join("units"), super::super::tests::capability(), "linux/amd64");
        let driver = std::sync::Arc::new(super::super::tests::Mounted(fake, std::sync::Mutex::new(None)));
        let image = podrunner_unit::UnitDriver::prepare(&*driver, IMAGE, "linux/amd64", false).unwrap();
        let exec = super::InUnit::new(driver, image);
        let mut job = podrunner_broker_fake::Job::scripts("a", &["true"]);
        job.overrides.insert("jobContainer".into(), serde_json::json!(IMAGE));
        let id = podrunner_schema::UnitId::parse("u-1").unwrap();
        // The socket directory does not exist: the relay fails. A directory
        // at the path of the socket: the bind fails.
        let sockets = dir.join("sockets");
        assert!(exec.hooks(&sockets, &job, &id, Some(ONE)).is_err());
        assert!(!super::logs_dir(&sockets).exists(), "no directory of the logs stays");
        std::fs::create_dir_all(&sockets).unwrap();
        std::fs::create_dir_all(sockets.join("ctl.sock/x")).unwrap();
        let e = exec.hooks(&sockets, &job, &id, Some(ONE)).err().unwrap();
        assert!(e.contains("the control socket"), "{e}");
        assert!(!super::logs_dir(&sockets).exists(), "the copy stays after a failed bind");
        let _ = std::fs::remove_dir_all(&dir);
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
