//! The executor of `--runner-dir` (T-089): the runner of an unpacked
//! release on the host, one job at a time, against a fake broker.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pod_actions_workflow::JobResult;
use podrunner_broker_fake::artifacts::ArtifactStore;
use podrunner_broker_fake::cache::CacheStore;
use podrunner_broker_fake::runner::start_runner_in;
use podrunner_broker_fake::{Broker, Job};
use podrunner_schema::JobId;

use super::{Live, needs_containers, pump, record};
use crate::drive::{End, Executor, result_of};

/// An unpacked release of the official runner, in a directory that the run
/// may change.
pub struct RunnerDir {
    pub dir: PathBuf,
    /// The work folder of the runner (`crate::work::host_work`, T-255).
    pub work: PathBuf,
    /// The log of each job, for the caller.
    pub logs: Vec<(JobId, String)>,
    /// The archives of the actions (`OWNER_REPO/SHA.tar.gz`, T-091).
    pub actions: Option<PathBuf>,
    /// The commit of each `OWNER/REPO@REF` of the run (T-118).
    pub pins: BTreeMap<String, String>,
    /// The artifact store of the run (T-119).
    pub artifacts: Option<Arc<ArtifactStore>>,
    /// The cache store (T-120).
    pub cache: Option<Arc<CacheStore>>,
    /// The issuer of the id-tokens of the run (T-140).
    pub oidc: Option<Arc<podrunner_broker_fake::oidc::Issuer>>,
    /// The live sink and the blobs of each job (T-134).
    pub live: Option<Live>,
    pub blobs: Vec<(JobId, Vec<(String, String)>)>,
    /// The tarball of the local tree for `actions/checkout` (T-092).
    pub checkout: Option<PathBuf>,
    /// `--export DIR`: the workspaces of each job go to `DIR/JOB` (T-138).
    pub export: Option<PathBuf>,
    /// The stop flag of the next job (T-132).
    stop: Option<crate::cancel::Stop>,
    /// No console of the runner on stderr (`-q`, `--json`, T-278).
    pub quiet: bool,
    /// The lock of the runner directory for this run (T-274).
    _lock: Arc<pod_core::lock::FileLock>,
}

/// A work directory out of the runner directory (`BASE/podrunner-work`,
/// T-255) goes with the run, and its parent when it is empty (T-331 F10).
impl Drop for RunnerDir {
    fn drop(&mut self) {
        if self.work != self.dir.join("_work") && crate::work::clear(&self.work).is_ok() {
            let _ = self.work.parent().map(std::fs::remove_dir);
        }
    }
}

/// The runner does not quote the path of a step script, so each `run:`
/// step fails in a runner directory whose path has a space (T-281).
pub fn space_refusal(dir: &std::path::Path) -> Option<String> {
    let abs = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    abs.to_string_lossy().contains(char::is_whitespace).then(|| {
        format!(
            "{}: the path of the runner directory has a space, and the runner then fails each run: step; fix: unpack the runner in a path with no space",
            abs.display()
        )
    })
}

/// Keep the newest 20 logs of `_diag`: the runner writes two for each job
/// and removes none (T-281).
fn prune_diag(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir.join("_diag")) else { return };
    let mut logs: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".log"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    logs.sort();
    let extra = logs.len().saturating_sub(20);
    for (_, path) in logs.into_iter().take(extra) {
        let _ = std::fs::remove_file(path);
    }
}

/// The file of the process group of the runner of the last job.
const GROUP: &str = ".podrunner-run.group";

/// End the runner that a run which died (a SIGKILL of podrunner) left in
/// `dir` (T-294): its process group, when its leader still works in `dir`.
fn end_stale(dir: &std::path::Path) {
    let Ok(text) = std::fs::read_to_string(dir.join(GROUP)) else { return };
    let Ok(pgid) = text.trim().parse::<i32>() else { return };
    let here = dir.canonicalize().ok();
    let cwd = std::fs::read_link(format!("/proc/{pgid}/cwd")).ok();
    if pgid > 1 && here.is_some() && cwd == here {
        eprintln!(
            "podrunner: {}: the runner of a run that died is still there (group {pgid}); it ends now",
            dir.display()
        );
        end_group(pgid as u32);
    }
    let _ = std::fs::remove_file(dir.join(GROUP));
}

/// The lock file of a run in a runner directory.
const LOCK: &str = ".podrunner-run.lock";

/// Take the runner directory `dir` for this run: a second run on it fails
/// at once, as two runners in one directory break each other (T-274). The
/// executors of one process (the routes of `--platform`, the runs of
/// `--watch`) share the lock, which holds until the process ends.
fn lock(dir: &std::path::Path) -> Result<Arc<pod_core::lock::FileLock>, String> {
    use pod_core::lock::{FileLock, Mode};
    static HELD: std::sync::Mutex<BTreeMap<PathBuf, Arc<FileLock>>> = std::sync::Mutex::new(BTreeMap::new());
    let key = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(f) = held.get(&key) {
        return Ok(f.clone());
    }
    let path = dir.join(LOCK);
    // `FileLock` opens a lock file of another user read-only, as flock
    // takes a read descriptor (pod-core, T-513).
    let lock = FileLock::try_lock(&path, Mode::Exclusive)
        .map_err(|e| format!("{e}; fix: the runner directory must be writable by you"))?
        .ok_or_else(|| {
            format!(
                "{}: another run of podrunner uses this runner directory; wait for it to end, or give each run its own --runner-dir",
                dir.display()
            )
        })?;
    let lock = Arc::new(lock);
    held.insert(key, lock.clone());
    Ok(lock)
}

impl RunnerDir {
    pub fn new(dir: PathBuf) -> Result<RunnerDir, String> {
        if !dir.join("run.sh").is_file() || !dir.join("bin/Runner.Listener").is_file() {
            return Err(format!("{}: no unpacked runner (run.sh, bin/Runner.Listener)", dir.display()));
        }
        if let Some(e) = space_refusal(&dir) {
            return Err(e);
        }
        let lock = lock(&dir)?;
        end_stale(&dir);
        Ok(RunnerDir {
            _lock: lock,
            quiet: false,
            work: crate::work::host_work(&dir),
            dir,
            logs: Vec::new(),
            actions: None,
            pins: BTreeMap::new(),
            artifacts: None,
            cache: None,
            oidc: None,
            live: None,
            blobs: Vec::new(),
            checkout: None,
            export: None,
            stop: None,
        })
    }
}

impl Executor for RunnerDir {
    fn take_logs(&mut self) -> Vec<(JobId, String)> {
        std::mem::take(&mut self.logs)
    }

    fn set_live(&mut self, live: Live) {
        self.live = Some(live);
    }

    fn set_stop(&mut self, stop: crate::cancel::Stop) {
        self.stop = Some(stop);
    }

    fn take_blobs(&mut self) -> Vec<(JobId, Vec<(String, String)>)> {
        std::mem::take(&mut self.blobs)
    }

    fn run(&mut self, job: &JobId, message: Job, limit: Duration) -> Result<End, String> {
        let end = self.job(job, message, limit);
        // The export of the workspaces, also of a failed job (T-138).
        if let Some(dir) = &self.export
            && let Err(e) = crate::work::export(&self.work, &dir.join(job.as_str()))
        {
            eprintln!("podrunner: {}: --export: {e}", job.as_str());
            match self.logs.iter_mut().rev().find(|(j, _)| j == job) {
                Some((_, log)) => log.push_str(&format!("\npodrunner: {e}")),
                None => self.logs.push((job.clone(), format!("podrunner: {e}"))),
            }
        }
        end
    }
}

impl RunnerDir {
    fn job(&mut self, job: &JobId, message: Job, limit: Duration) -> Result<End, String> {
        if needs_containers(&message) {
            return Err("the job needs containers: run it in a unit (--podvm, or --microvm with --engine), not on a runner directory".into());
        }
        if let Some(e) = super::docker::refusal(
            &message,
            &super::docker::Sources {
                store: self.actions.as_deref(),
                pins: &self.pins,
                tree: self.checkout.as_deref(),
            },
        ) {
            return Err(e);
        }
        let broker = Broker::start().map_err(super::broker_error)?;
        // The pins also name the trees of the checkouts in actions (T-713).
        broker.serve_actions(self.actions.as_deref(), self.pins.clone());
        if let Some(store) = &self.artifacts {
            broker.serve_artifacts(store.clone());
        }
        if let Some(store) = &self.cache {
            broker.serve_cache(store.clone());
        }
        if let Some(issuer) = &self.oidc {
            broker.serve_oidc(issuer.clone());
        }
        if let Some(tree) = &self.checkout {
            broker.serve_checkout(tree);
        }
        broker.queue(message);
        // Each job starts in a fresh work directory (T-146).
        crate::work::clear(&self.work)?;
        // The start of the runner clears the pages and the blocks that a
        // signal left (`podrunner_broker_fake::runner::clear_diag`, T-273).
        prune_diag(&self.dir);
        // A scratch home for each job: a step must not change the home of
        // the user (T-266); the next job clears it with the work directory.
        let home = self.work.join("_home");
        std::fs::create_dir_all(&home).map_err(|e| format!("{}: {e}", home.display()))?;
        let mut env = host_env(&home, std::env::vars());
        super::proxy::host_trust(&home, &mut env);
        let mut child = start_runner_in(&self.dir, &self.work, &broker, &env, !self.quiet)?;
        let _ = std::fs::write(self.dir.join(GROUP), child.id().to_string());
        // A third signal ends the group at once (T-636).
        let _held = crate::cancel::hold_group(child.id());
        // The limit of the job (`timeout-minutes`, T-115): at its end, the
        // whole process group of the runner ends, as the unit of `InUnit`.
        let end = Instant::now() + limit;
        let status = loop {
            if let Some(s) = child.try_wait().map_err(|e| format!("the runner: {}", e.kind()))? {
                break Some(s);
            }
            // A signal ends the job as its limit does (T-141).
            if Instant::now() >= end || crate::cancel::stopped(&self.stop) {
                break None;
            }
            pump(&self.live, job, &broker);
            std::thread::sleep(Duration::from_millis(100));
        };
        let Some(status) = status else {
            end_group(child.id());
            let _ = child.wait();
            {
                let log = record(&self.live, &mut self.blobs, job, &broker);
                self.logs.push((job.clone(), log));
            }
            if crate::cancel::stopped(&self.stop) {
                return Ok(End { result: JobResult::Cancelled, outputs: vec![] });
            }
            return Err(format!("the job ran past its limit of {} s", limit.as_secs()));
        };
        if let Some(why) = signal_reason(&status) {
            {
                let log = record(&self.live, &mut self.blobs, job, &broker);
                self.logs.push((job.clone(), log));
            }
            return Err(why);
        }
        // The runner sends `completejob` before it ends; give the broker
        // time to record it.
        let t = Instant::now();
        while broker.completions().is_empty() && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        {
            let log = record(&self.live, &mut self.blobs, job, &broker);
            self.logs.push((job.clone(), log));
        }
        let done = broker.completions();
        let Some(end) = done.first() else {
            return Err("the runner ended with no result".into());
        };
        Ok(End { result: result_of(&end.conclusion), outputs: end.outputs.clone() })
    }
}

/// The variables of the user that a job on a runner directory takes
/// (T-265): `PATH`, the CA files and the proxy. The broker is on the
/// loopback, so `NO_PROXY` always holds the loopback. The CA files are the
/// ten variables of `pod_broker::ca::env` (T-576) and `SSL_CERT_DIR`.
pub const PASSED: &[&str] = &[
    "PATH",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
    "GIT_SSL_CAINFO",
    "REQUESTS_CA_BUNDLE",
    "CURL_CA_BUNDLE",
    "AWS_CA_BUNDLE",
    "DENO_CERT",
    "HEX_CACERTS_PATH",
    "CLOUDSDK_CORE_CUSTOM_CA_CERTS_FILE",
    "CARGO_HTTP_CAINFO",
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "NODE_USE_ENV_PROXY",
];

/// The variable that makes the `fetch` of Node use the proxy variables
/// (Node 22.21 and 24.5 or later). Actions that call the API of GitHub by
/// `fetch` connect direct without it, and a proxy-only host refuses them
/// (T-299). The value of the user has priority.
pub(crate) fn node_proxy(vars: &[(&str, String)]) -> Option<(&'static str, String)> {
    let proxy = vars.iter().any(|(k, _)| k.to_ascii_lowercase().ends_with("_proxy") && *k != "NODE_USE_ENV_PROXY");
    let set = vars.iter().any(|(k, _)| *k == "NODE_USE_ENV_PROXY");
    (proxy && !set).then(|| ("NODE_USE_ENV_PROXY", "1".to_string()))
}

/// The environment of the runner of one job: the variables of `PASSED`
/// from `vars`, `HOME` and `NO_PROXY`.
pub(crate) fn host_env(
    home: &std::path::Path,
    vars: impl Iterator<Item = (String, String)>,
) -> Vec<(&'static str, String)> {
    let mut vars: std::collections::BTreeMap<String, String> = vars.collect();
    super::proxy::node_ca(&mut vars);
    let mut out: Vec<(&'static str, String)> =
        PASSED.iter().filter_map(|k| vars.get(*k).filter(|v| !v.is_empty()).map(|v| (*k, v.clone()))).collect();
    out.extend(node_proxy(&out));
    out.push(("HOME", home.display().to_string()));
    let (no_proxy, dropped) = super::noproxy::job_list(&vars);
    super::noproxy::name_dropped(&dropped);
    out.push(("NO_PROXY", no_proxy.clone()));
    out.push(("no_proxy", no_proxy));
    out
}

/// End the process group of `leader`: TERM, then KILL after 5 s.
fn end_group(leader: u32) {
    let Ok(pgid) = i32::try_from(leader) else { return };
    // SAFETY: kill of a process group that this process made.
    unsafe { libc::kill(-pgid, libc::SIGTERM) };
    let t = Instant::now();
    // SAFETY: a signal 0 checks that the group still has a process.
    while t.elapsed() < Duration::from_secs(5) && unsafe { libc::kill(-pgid, 0) } == 0 {
        std::thread::sleep(Duration::from_millis(100));
    }
    // SAFETY: as above.
    unsafe { libc::kill(-pgid, libc::SIGKILL) };
}

/// The reason of a runner that a signal ended, with the cause and the fix
/// of SIGXFSZ (a limit of file size, T-144).
fn signal_reason(status: &std::process::ExitStatus) -> Option<String> {
    podrunner_unit::signal::ended_by(status).map(|s| format!("the runner {s}"))
}

#[cfg(test)]
mod env_tests {
    use super::host_env;

    /// The job takes `PATH`, the CA and the proxy of the user, a scratch
    /// home, and the loopback stays off the proxy (T-265, T-266).
    #[test]
    fn a_job_on_a_runner_directory_takes_the_path_the_ca_and_the_proxy() {
        let vars = [
            ("PATH", "/opt/x/bin:/usr/local/bin:/usr/bin:/bin"),
            ("HTTPS_PROXY", "http://proxy.example:3128"),
            ("SSL_CERT_FILE", "/etc/ca.pem"),
            ("NO_PROXY", "intra.example,::,10.0.0.0/8,localhost"),
            ("AWS_SECRET_ACCESS_KEY", "made-up"),
            ("HOME", "/home/user"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let env = host_env(std::path::Path::new("/r/_work/_home"), vars.into_iter());
        let get = |k: &str| env.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("PATH"), Some("/opt/x/bin:/usr/local/bin:/usr/bin:/bin"));
        assert_eq!(get("HTTPS_PROXY"), Some("http://proxy.example:3128"));
        assert_eq!(get("SSL_CERT_FILE"), Some("/etc/ca.pem"));
        assert_eq!(get("HOME"), Some("/r/_work/_home"));
        assert_eq!(get("NO_PROXY"), Some("127.0.0.1,localhost,::1,intra.example,10.0.0.0/8"));
        assert_eq!(get("AWS_SECRET_ACCESS_KEY"), None, "only the listed variables pass");
        assert_eq!(get("NODE_USE_ENV_PROXY"), Some("1"), "fetch of Node uses the proxy (T-299)");
    }

    /// With no proxy, Node gets no proxy variable; a value of the user stays.
    #[test]
    fn node_takes_the_proxy_only_with_a_proxy() {
        let home = std::path::Path::new("/h");
        let env = host_env(home, [("PATH".to_string(), "/bin".to_string())].into_iter());
        assert!(env.iter().all(|(k, _)| *k != "NODE_USE_ENV_PROXY"));
        let vars =
            [("https_proxy", "http://p:1"), ("NODE_USE_ENV_PROXY", "0")].map(|(k, v)| (k.to_string(), v.to_string()));
        let env = host_env(home, vars.into_iter());
        let node: Vec<_> = env.iter().filter(|(k, _)| *k == "NODE_USE_ENV_PROXY").collect();
        assert_eq!(node, [&("NODE_USE_ENV_PROXY", "0".to_string())]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::lock::{FileLock, Mode};

    /// T-687: the lock of a runner directory holds until the process ends,
    /// also after the last run of the directory dropped its handle
    /// (`FileLock` unlocks at its drop, so `HELD` must keep the value).
    #[test]
    fn the_lock_outlives_the_last_handle_of_its_directory() {
        let dir = std::env::temp_dir().join(format!("podrunner-host-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        drop(lock(&dir).unwrap());
        let probe = FileLock::try_lock_existing(&dir.join(LOCK), Mode::Exclusive).unwrap();
        assert!(probe.is_none(), "the lock of the directory is free after the drop of its last handle");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
