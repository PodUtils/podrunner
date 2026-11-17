//! The canned-message fake broker of GitHub Actions (GitHub #38, level 2):
//! the official runner runs a job offline. It serves the routes that runner
//! v2.338.2 calls with a JIT configuration of a scale set: the token of the
//! OAuth client, the broker session and its messages, and the run service
//! (`acquirejob`, `renewjob`, `completejob`). It gives a job only to the
//! runner of its own key (the module `auth`, T-406). The fakes of GitHub stay in
//! podrunner (PodUtils/podcrates@T-106).

pub mod actions;
mod auth;
mod listen;
mod results;
use listen::Wake;
pub use listen::bind_loopback;
pub mod artifacts;
pub mod cache;
pub mod corpus;
pub mod feed;
pub mod files;
pub mod hold;
pub mod message;
pub mod mirror;
pub mod oidc;
pub mod runner;

use std::collections::VecDeque;
use std::io::BufReader;
use std::net::TcpStream;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use podrunner_hook::http::{read_request_with, write_response};
use serde_json::{Value, json};

pub use message::{Job, Step};

/// The end of a job, as the runner reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub request: u64,
    /// `succeeded`, `failed`, `cancelled`.
    pub conclusion: String,
    /// The names of the steps that the runner reports.
    pub steps: Vec<String>,
    /// The outputs of the job (`jobOutputs`, evaluated by the runner).
    pub outputs: Vec<(String, String)>,
}

#[derive(Default)]
struct State {
    /// The uploaded logs, by blob name (`job`, `step-ID`, `summary-ID`).
    blobs: std::collections::BTreeMap<String, Vec<u8>>,
    /// How much of each blob [`Broker::take_new`] gave (T-134).
    read: std::collections::BTreeMap<String, usize>,
    /// The lines of the live feed not given yet, and each step that the
    /// feed gave lines of: its blob is not given again (T-310).
    fed: Vec<String>,
    fed_steps: std::collections::BTreeSet<String>,
    /// The bodies of `WorkflowStepsUpdate`, in order.
    step_updates: Vec<Value>,
    queue: VecDeque<(u64, Job)>,
    acquired: Vec<(u64, Job)>,
    done: Vec<Completion>,
    /// Each request line, for a test that looks at the routes.
    seen: Vec<String>,
    next: u64,
    /// Each long poll answers "version too old" (T-066).
    too_old: bool,
    /// The archives of the actions (T-091).
    actions: Option<std::path::PathBuf>,
    /// The commit of each `OWNER/REPO@REF` of the run (T-118).
    pins: std::collections::BTreeMap<String, String>,
    /// The tarball of the local tree (T-092).
    tree: Option<std::path::PathBuf>,
    /// The artifact store of the run (T-119).
    artifacts: Option<Arc<artifacts::ArtifactStore>>,
    /// The cache store (T-120).
    cache: Option<Arc<cache::CacheStore>>,
    /// Each request with no route, with its service (T-145).
    missing: Vec<String>,
    /// The issuer of the id-tokens of the run (T-140).
    oidc: Option<Arc<oidc::Issuer>>,
    /// The registry of the pinned images for the engine of a guest (T-157).
    mirror: Option<Arc<mirror::Mirror>>,
    /// The tokens and the runner keys of this broker (T-406).
    auth: auth::Auth,
}

/// The service of a route that this broker does not serve, and the entry
/// that gives it, when it is known (T-145).
pub fn service_of(path: &str) -> Option<&'static str> {
    let known: [(&str, &str); 5] = [
        ("ArtifactService", "the ArtifactService of the results API (T-119)"),
        ("CacheService", "the CacheService of the results API (T-120)"),
        (
            "_apis/artifactcache",
            "the legacy cache routes, which GitHub turned off; actions/cache@v4 takes the CacheService (T-120)",
        ),
        ("_apis/pipelines/workflows", "the legacy artifact routes of v3, which GitHub turned off (T-119)"),
        ("idtoken", "the OIDC id-token of the job (T-140)"),
    ];
    known.iter().find(|(k, _)| path.contains(k)).map(|(_, s)| *s)
}

/// The largest body of a request: the cache client sends an archive under
/// 128 MiB in one PUT.
const BODY_MAX: usize = 160 << 20;

/// A fake broker on the loopback.
pub struct Broker {
    pub base: String,
    state: Arc<Mutex<State>>,
    /// The end of the listener: set at drop, then a connect wakes its loop,
    /// which closes the listener (#221).
    stop: Arc<std::sync::atomic::AtomicBool>,
    wake: Wake,
}

/// The most lines of `seen` and of `missing`.
const LIST_MAX: usize = 10_000;

/// How long a long poll of `/message` waits for a job.
const POLL: Duration = Duration::from_secs(2);

impl Broker {
    /// Listen on a free port of the loopback.
    pub fn start() -> std::io::Result<Broker> {
        Broker::start_seen_at("127.0.0.1")
    }

    /// Listen on a free port of the loopback, and give its clients the
    /// address `host` in each URL: a client behind a proxy names the
    /// broker so.
    pub fn start_seen_at(host: &str) -> std::io::Result<Broker> {
        let (listener, port) = bind_loopback()?;
        let base = format!("http://{host}:{port}/");
        let state: Arc<Mutex<State>> = Arc::default();
        let (s, b) = (state.clone(), base.clone());
        let stop: Arc<std::sync::atomic::AtomicBool> = Arc::default();
        let end = stop.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                if end.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let (s, b) = (s.clone(), b.clone());
                std::thread::spawn(move || serve(conn, &s, &b));
            }
        });
        Ok(Broker { base, state, stop, wake: Wake::Tcp(port) })
    }

    /// Listen on the unix socket `path`, and give its clients the address
    /// `host` in each URL: a host that refuses each AF_INET bind serves a
    /// guest through its front (T-220).
    pub fn start_unix(path: &std::path::Path, host: &str) -> std::io::Result<Broker> {
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path)?;
        let base = format!("http://{host}/");
        let state: Arc<Mutex<State>> = Arc::default();
        let (s, b) = (state.clone(), base.clone());
        let stop: Arc<std::sync::atomic::AtomicBool> = Arc::default();
        let end = stop.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                if end.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let (s, b) = (s.clone(), b.clone());
                std::thread::spawn(move || serve(conn, &s, &b));
            }
        });
        Ok(Broker { base, state, stop, wake: Wake::Unix(path.to_path_buf()) })
    }

    /// The files of a runner named `name` (`files::runner_files`), with a
    /// new key that this broker admits: only that runner gets a job.
    pub fn runner_files(&self, name: &str) -> Vec<(String, String)> {
        let key = runner::rsaparams();
        self.state.lock().expect("the broker").auth.admit(&key).expect("a key of runner::rsaparams");
        files::runner_files(&self.base, name, &key)
    }

    /// Offer `job` to the next long poll; its request id.
    pub fn queue(&self, job: Job) -> u64 {
        let mut st = self.state.lock().expect("the broker");
        st.next += 1;
        let id = st.next;
        st.queue.push_back((id, job));
        id
    }

    /// Answer each long poll of `/message` with the error of the broker
    /// for a runner whose version is too old: the runner then exits 7 when
    /// `ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE` is `1`.
    pub fn refuse_the_version(&self) {
        self.state.lock().expect("the broker").too_old = true;
    }

    /// Serve the tarball of each action from `dir` (`actions`, T-091), and
    /// resolve each `OWNER/REPO@REF` from `pins` (T-118).
    pub fn serve_actions(&self, dir: &std::path::Path, pins: std::collections::BTreeMap<String, String>) {
        let mut st = self.state.lock().expect("the broker");
        st.actions = Some(dir.to_path_buf());
        st.pins = pins;
    }

    /// Serve the ArtifactService and its blobs over `store`, the store of
    /// the run (T-119).
    pub fn serve_artifacts(&self, store: Arc<artifacts::ArtifactStore>) {
        self.state.lock().expect("the broker").artifacts = Some(store);
    }

    /// Serve the id-tokens of the jobs with the key of `issuer` (T-140).
    pub fn serve_oidc(&self, issuer: Arc<oidc::Issuer>) {
        self.state.lock().expect("the broker").oidc = Some(issuer);
    }

    /// Serve the pinned images of the run to the engine of a guest (T-157).
    pub fn serve_mirror(&self, m: Arc<mirror::Mirror>) {
        self.state.lock().expect("the broker").mirror = Some(m);
    }

    /// Serve the CacheService and its blobs over `store` (T-120).
    pub fn serve_cache(&self, store: Arc<cache::CacheStore>) {
        self.state.lock().expect("the broker").cache = Some(store);
    }

    /// Serve `tarball` as the tree of the stand-in of `actions/checkout`
    /// (`actions::CHECKOUT`, T-092).
    pub fn serve_checkout(&self, tarball: &std::path::Path) {
        self.state.lock().expect("the broker").tree = Some(tarball.to_path_buf());
    }

    /// The jobs that ended.
    pub fn completions(&self) -> Vec<Completion> {
        self.state.lock().expect("the broker").done.clone()
    }

    /// The logs that the Worker uploaded: the job log and each step log,
    /// as text, with no summary.
    pub fn logs(&self) -> String {
        let st = self.state.lock().expect("the broker");
        let text = |b: &Vec<u8>| String::from_utf8_lossy(b).replace('\u{feff}', "");
        // The log of the whole job has each step in order; the step logs
        // (named by a random id) only when the runner sent no job log (T-282).
        let whole: Vec<String> = st
            .blobs
            .iter()
            .filter(|(k, _)| !k.starts_with("summary-") && !k.starts_with("step-"))
            .map(|(_, b)| text(b))
            .collect();
        match whole.is_empty() {
            false => whole.join("\n"),
            true => st
                .blobs
                .iter()
                .filter(|(k, _)| k.starts_with("step-"))
                .map(|(_, b)| text(b))
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// Each blob by its name (`step-ID`, `summary-ID`, the job log), as text
    /// (T-134).
    pub fn blobs(&self) -> Vec<(String, String)> {
        let st = self.state.lock().expect("the broker");
        st.blobs.iter().map(|(k, b)| (k.clone(), String::from_utf8_lossy(b).into_owned())).collect()
    }

    /// The text that each log blob got since the last call: the live lines
    /// of a run (T-134).
    pub fn take_new(&self) -> Vec<(String, String)> {
        let mut st = self.state.lock().expect("the broker");
        let mut out = Vec::new();
        // The step logs only: the log of the whole job repeats each line
        // (T-282).
        if !st.fed.is_empty() {
            let mut text = std::mem::take(&mut st.fed).join("\n");
            text.push('\n');
            out.push(("feed".to_string(), text));
        }
        let names: Vec<String> = st.blobs.keys().filter(|k| k.starts_with("step-")).cloned().collect();
        for name in names {
            let len = st.blobs[&name].len();
            let from = st.read.get(&name).copied().unwrap_or(0).min(len);
            let fed = name.strip_prefix("step-").is_some_and(|id| st.fed_steps.contains(&id.to_ascii_lowercase()));
            if fed {
                st.read.insert(name, len);
            } else if from < len {
                out.push((name.clone(), String::from_utf8_lossy(&st.blobs[&name][from..]).replace('\u{feff}', "")));
                st.read.insert(name, len);
            }
        }
        out
    }

    /// The step updates of the Worker.
    pub fn step_updates(&self) -> Vec<Value> {
        self.state.lock().expect("the broker").step_updates.clone()
    }

    /// Each request with no route: the method, the path with no query, and
    /// its service when it is known (T-145).
    pub fn missing(&self) -> Vec<String> {
        self.state.lock().expect("the broker").missing.clone()
    }

    /// The request line of each call.
    pub fn seen(&self) -> Vec<String> {
        self.state.lock().expect("the broker").seen.clone()
    }
}

fn reply(json: &Value) -> (u16, Vec<u8>) {
    (200, json.to_string().into_bytes())
}

/// A connection of a client: TCP or a unix socket.
trait Conn: std::io::Read + std::io::Write + Sized {
    fn dup(&self) -> std::io::Result<Self>;
}

impl Conn for TcpStream {
    fn dup(&self) -> std::io::Result<Self> {
        self.try_clone()
    }
}

impl Conn for UnixStream {
    fn dup(&self) -> std::io::Result<Self> {
        self.try_clone()
    }
}

fn serve<C: Conn>(conn: C, state: &Mutex<State>, base: &str) {
    let Ok(mut w) = conn.dup() else { return };
    let mut req = match read_request_with(&mut BufReader::new(conn), BODY_MAX) {
        Ok(r) => r,
        // An answer, not a closed connection: a client retries a closed one
        // for minutes.
        Err(e) => {
            let _ = write_response(&mut w, e.0, &[], e.1.as_bytes());
            return;
        }
    };
    // A request of a proxy (the front of a guest) can name the absolute
    // form `http://HOST/PATH`: the routes take `/PATH`.
    let mut form = "";
    if let Some(rest) = req.path.strip_prefix("http://").or_else(|| req.path.strip_prefix("https://")) {
        req.path = rest.find('/').map_or_else(|| "/".to_string(), |i| rest[i..].to_string());
        form = " (absolute form)";
    }
    let mut path = req.path.split('?').next().unwrap_or("").to_string();
    // A hold names the hold key of this broker (T-406); the key goes
    // into no log.
    match state.lock().expect("the broker").auth.hold_path(&path) {
        Some(Ok(p)) => path = p,
        Some(Err(())) => {
            let _ = write_response(&mut w, 401, &[], b"the broker gives a hold only with its key");
            return;
        }
        None => {}
    }
    {
        // A bound: each local process can send requests (T-406).
        let mut st = state.lock().expect("the broker");
        if st.seen.len() < LIST_MAX {
            st.seen.push(format!("{} {path}{form}", req.method));
        }
    }
    let refused = auth::need(&req.method, &path)
        .is_some_and(|n| !state.lock().expect("the broker").auth.allows(n, req.authorization.as_deref()));
    if refused {
        let _ = write_response(&mut w, 401, &[], b"the broker gives this route only to its own runner");
        return;
    }
    if let (true, Some(key)) = (req.method == "GET" && path == feed::PATH, &req.ws_key) {
        if feed::handshake(&mut w, key).is_ok() {
            feed::read_messages(&mut w, |step, lines| {
                let mut st = state.lock().expect("the broker");
                st.fed_steps.insert(step.to_string());
                st.fed.extend(lines);
            });
        }
        return;
    }
    let images = state.lock().expect("the broker").mirror.clone();
    if let Some((status, out, headers)) = images.and_then(|m| m.route(&req.method, &path)) {
        let _ = mirror::write(&mut w, &req.method, status, &headers, &out);
        return;
    }
    let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
    let base = base.trim_end_matches('/');
    let base = &format!("{base}/");
    let (dir, pins, tree, store, cache) = {
        let st = state.lock().expect("the broker");
        (st.actions.clone(), st.pins.clone(), st.tree.clone(), st.artifacts.clone(), st.cache.clone())
    };
    let cached = match cache {
        Some(c) => c.route(&req.method, &req.path, &req.body, req.range.as_deref(), base),
        None => cache::off(&req.method, &req.path),
    };
    if let Some((status, out, mut headers)) = cached {
        headers.push(("Content-Type", "application/json".to_string()));
        let _ = write_response(&mut w, status, &headers, &out);
        return;
    }
    let id_token = {
        let st = state.lock().expect("the broker");
        let claims = st.acquired.iter().rev().find_map(|(_, j)| j.id_token.clone());
        st.oidc.clone().and_then(|i| oidc::route(&req.method, &req.path, &i, claims.as_ref()))
    };
    let actions = actions::route(&req.method, &path, &body, base, dir.as_deref(), &pins, tree.as_deref())
        .or(id_token)
        .or_else(|| hold::route(&req.method, &path))
        .or_else(|| store.and_then(|s| s.route(&req.method, &req.path, &req.body, base)))
        .or_else(|| results::route(&req.method, &path, &req.path, &body, &req.body, base, state));
    let (status, out) = match (req.method.as_str(), path.as_str()) {
        _ if actions.is_some() => actions.expect("an answer"),
        ("POST", "/_apis/oauth2/token") => match state.lock().expect("the broker").auth.grant(&req.body) {
            Some(t) => reply(&json!({ "access_token": t, "token_type": "bearer", "expires_in": 3600 })),
            None => (401, br#"{"error":"invalid_client"}"#.to_vec()),
        },
        ("POST", "/session") => {
            let mut session = body;
            session["sessionId"] = json!("11111111-1111-1111-1111-111111111111");
            reply(&session)
        }
        ("DELETE", "/session") => (200, Vec::new()),
        ("GET", "/message") if state.lock().expect("the broker").too_old => (
            403,
            json!({
                "source": "actions-broker-listener", "errorKind": "RunnerVersionTooOld", "statusCode": 403,
                "errorMessage": "the runner version is too old",
            })
            .to_string()
            .into_bytes(),
        ),
        ("GET", "/message") => {
            let end = Instant::now() + POLL;
            loop {
                let next = state.lock().expect("the broker").queue.pop_front();
                if let Some((id, job)) = next {
                    state.lock().expect("the broker").acquired.push((id, job));
                    break reply(&message::job_request(base, id));
                }
                if Instant::now() >= end {
                    break (202, Vec::new());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        ("POST", "/acquirejob") => {
            let id: u64 = body["jobMessageId"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
            let st = state.lock().expect("the broker");
            match st.acquired.iter().find(|(i, _)| *i == id).or(st.acquired.last()) {
                Some((i, job)) => reply(&message::job_message_with(base, *i, job, &st.auth.job, &st.auth.hold)),
                None => (404, Vec::new()),
            }
        }
        ("POST", "/renewjob") => reply(&json!({ "lockedUntil": "2099-01-01T00:00:00Z" })),
        ("POST", "/completejob") => {
            let steps = body["stepResults"]
                .as_array()
                .map(|a| a.iter().filter_map(|s| s["name"].as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let mut st = state.lock().expect("the broker");
            let request = st.acquired.last().map_or(0, |(i, _)| *i);
            let conclusion = body["conclusion"].as_str().unwrap_or("").to_string();
            let outputs = body["outputs"]
                .as_object()
                .map(|o| o.iter().map(|(k, v)| (k.clone(), v["value"].as_str().unwrap_or("").to_string())).collect())
                .unwrap_or_default();
            st.done.push(Completion { request, conclusion, steps, outputs });
            (200, Vec::new())
        }
        (method, p) => {
            let line = match service_of(p) {
                Some(s) => format!("{method} {p} ({s})"),
                None => format!("{method} {p}"),
            };
            let mut st = state.lock().expect("the broker");
            if st.missing.len() < LIST_MAX {
                st.missing.push(line);
            }
            (404, Vec::new())
        }
    };
    let headers = [("Content-Type", "application/json".to_string())];
    let _ = write_response(&mut w, status, &headers, &out);
}

#[cfg(test)]
mod tests;
