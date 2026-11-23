//! The egress module of podrunner (GitHub #24): the one way out of each
//! unit. It hosts `pod-broker` of podcrates on a runtime of its own and
//! gives each unit a listener of its job alone (`serve_unix_job`): the
//! socket is the identity of the job, so no token reaches the unit, and
//! `Proxy-Authorization` of another job gets 407. The unit reaches it as
//! `http://unix:/PATH`, the form of `podnet pod -x`.
//!
//! The broker of a host has its TLS end ([`tls`]): a job holds the
//! placeholder of a credential, and the broker gives the value of the
//! secret to the hosts of that credential only.

use std::collections::HashMap;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pod_broker::{Broker, Load, Policy, Token};
use pod_core::{BoxFuture, Class, Failure, Found, Resolve, Source, Stage};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

pub mod denials;
pub mod dns;
pub mod lines;
pub mod tls;

pub use pod_broker::Secrets;

/// What [`Egress::open_with`] gives for a job.
#[derive(Debug, Default)]
pub struct Opened {
    /// The count of allow rules under an open suffix (`JobInfo::flagged`).
    pub flagged: usize,
    /// The CA bundle that the job trusts (PEM: the CA of its job, then the
    /// roots of the host), when the broker ends its TLS: a credential or a
    /// rule with a path.
    pub ca_bundle: Option<String>,
    /// The same bundle as a PKCS #12 trust store for a JVM (T-577), which
    /// reads no PEM file.
    pub trust_store: Option<Vec<u8>>,
}

/// A failure of the egress module, with no byte of a policy or a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressError(pub String);

impl std::fmt::Display for EgressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EgressError {}

/// The resolver of the system, through the runtime (getaddrinfo). The
/// broker asks it once for each name and checks each address.
pub struct SystemResolve;

impl Resolve for SystemResolve {
    fn label(&self) -> String {
        "system".into()
    }

    fn resolve_from<'a>(
        &'a self,
        host: &'a str,
        port: u16,
        _deadline: tokio::time::Instant,
        _after: Option<Source>,
    ) -> BoxFuture<'a, Result<Found, Failure>> {
        Box::pin(async move {
            let addrs: Vec<_> = tokio::net::lookup_host((host, port))
                .await
                .map_err(|_| Failure::new(Stage::Dns, Class::NotFound, "the system resolver has no address"))?
                .collect();
            if addrs.is_empty() {
                return Err(Failure::new(Stage::Dns, Class::NotFound, "the system resolver has no address"));
            }
            Ok(Found { addrs, source: Source { rank: 0, name: "system" }, more: false })
        })
    }
}

/// The socket of a job and the task that serves it.
type Listener = (PathBuf, JoinHandle<std::io::Result<()>>);

/// The egress module of a host.
pub struct Egress {
    rt: Runtime,
    broker: Arc<Broker>,
    open: Mutex<HashMap<String, Listener>>,
    alive: Arc<AtomicBool>,
    denials: Arc<denials::Denials>,
    /// The roots of the bundle of a job (`tls::HostTls::roots_pem`).
    roots: Option<Vec<u8>>,
}

/// A token that no unit sees: the listener is the identity of the job.
fn token() -> Token {
    let mut b = [0u8; 16];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b));
    Token::new(b.iter().map(|x| format!("{x:02x}")).collect::<String>())
}

impl Egress {
    /// The module over `broker`, on a runtime of two threads, with no TLS
    /// end: a credential or a path rule is refused at the load.
    pub fn new(broker: Broker) -> Result<Egress, EgressError> {
        Egress::build(broker, None)
    }

    /// The module over `broker` with the TLS end of `tls`.
    pub fn with_tls(broker: Broker, tls: tls::HostTls) -> Result<Egress, EgressError> {
        let end = pod_broker::TlsEnd::new(tls::provider(), tls.trust);
        Egress::build(broker.with_tunnel_end(Arc::new(end)), tls.roots_pem)
    }

    fn build(broker: Broker, roots: Option<Vec<u8>>) -> Result<Egress, EgressError> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("podrunner-egress")
            .enable_all()
            .build()
            .map_err(|e| EgressError(format!("the runtime of egress: {}", e.kind())))?;
        let denials = Arc::new(denials::Denials::default());
        Ok(Egress {
            rt,
            broker: Arc::new(broker.with_audit(denials.clone())),
            open: Mutex::new(HashMap::new()),
            alive: Arc::new(AtomicBool::new(true)),
            denials,
            roots,
        })
    }

    /// The module of a host: the resolver `resolve` ([`dns::resolver`]),
    /// and a direct way out (or through `proxy`). With `names_only`, the
    /// broker sends the checked name to `proxy`, which takes names only.
    /// The broker has the TLS end with the trust of this host.
    pub fn host(
        proxy: Option<pod_proxy::Proxy>,
        names_only: bool,
        resolve: Arc<dyn Resolve>,
    ) -> Result<Egress, EgressError> {
        let dial = pod_broker::upstream("podrunner", proxy, resolve.clone());
        let settings = pod_broker::Settings { names_upstream: names_only, ..Default::default() };
        Egress::with_tls(Broker::new(settings, resolve, dial), tls::HostTls::of_host()?)
    }

    /// Ask the way out whether it refuses a name of a non-public address
    /// (the canary of `pod-broker`): `refuses-non-public`,
    /// `takes-non-public` or `unknown`. Only `refuses-non-public` keeps an
    /// `enforced` hold when the broker sends names (T-048).
    pub fn check_upstream(&self) -> &'static str {
        self.rt.block_on(self.broker.check_upstream()).word()
    }

    /// Load the policy of `job` (JSON of `broker-policy.v1`) and serve it
    /// alone on a new unix socket at `socket` (mode 0600). A job that is
    /// open already is loaded again with the new policy and keeps its
    /// listener.
    /// It gives the count of allow rules under an open suffix
    /// (`JobInfo::flagged` of `pod-broker`).
    pub fn open(&self, job: &str, policy: &str, socket: &Path, ttl: Duration) -> Result<usize, EgressError> {
        self.open_with(job, policy, Secrets::new(), socket, ttl).map(|o| o.flagged)
    }

    /// [`Egress::open`] with the values of the secrets that the credentials
    /// of `policy` name. No error text holds a value.
    pub fn open_with(
        &self,
        job: &str,
        policy: &str,
        secrets: Secrets,
        socket: &Path,
        ttl: Duration,
    ) -> Result<Opened, EgressError> {
        if !self.alive() {
            return Err(EgressError("egress is down".into()));
        }
        let policy = Policy::from_json(policy).map_err(|e| EgressError(format!("the policy of {job}: {}", e.class)))?;
        let mut load = Load::new(job, token(), ttl, policy);
        load.secrets = secrets;
        // The words of a usage failure of the load name a secret, never its
        // value (`pod-broker`: no text of an error holds one).
        let info =
            self.broker.load(load).map_err(|e| EgressError(format!("the load of {job}: {}: {}", e.class, e.text())))?;
        let (ca_bundle, trust_store) = match &info.ca_pem {
            Some(ca) => {
                let roots = self.roots.as_deref();
                match tls::bundle(job, ca, roots).and_then(|b| Ok((b, tls::trust_store(job, ca, roots)?))) {
                    Ok((b, s)) => (Some(b), Some(s)),
                    Err(e) => {
                        self.broker.drop_job(job);
                        return Err(e);
                    }
                }
            }
            None => (None, None),
        };
        let opened = Opened { flagged: info.flagged.len(), ca_bundle, trust_store };
        let mut open = self.open.lock().expect("the listeners");
        if open.contains_key(job) {
            return Ok(opened);
        }
        let _ = std::fs::remove_file(socket);
        let std_listener =
            std::os::unix::net::UnixListener::bind(socket).map_err(|e| EgressError(format!("bind: {}", e.kind())))?;
        std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| EgressError(format!("the mode of the socket: {}", e.kind())))?;
        std_listener.set_nonblocking(true).map_err(|e| EgressError(format!("the socket: {}", e.kind())))?;
        let _guard = self.rt.enter();
        let listener = tokio::net::UnixListener::from_std(std_listener)
            .map_err(|e| EgressError(format!("the socket: {}", e.kind())))?;
        let task = self.rt.spawn(self.broker.clone().serve_unix_job(job.to_string(), listener));
        open.insert(job.to_string(), (socket.to_path_buf(), task));
        Ok(opened)
    }

    /// Drop the job: its listener and each of its connections end, and the
    /// socket file goes.
    pub fn close(&self, job: &str) -> bool {
        let dropped = self.broker.drop_job(job);
        if let Some((socket, task)) = self.open.lock().expect("the listeners").remove(job) {
            task.abort();
            let _ = std::fs::remove_file(socket);
        }
        dropped
    }

    /// Write no line on stderr for each failed connection (the local mode).
    pub fn quiet(self) -> Self {
        self.denials.quiet();
        self
    }

    /// The line of each connection that ended since the last call, with the
    /// job (the unit) of its listener (T-486).
    pub fn take_lines(&self) -> Vec<(String, serde_json::Map<String, serde_json::Value>)> {
        self.denials.lines.take()
    }

    /// `HOST:PORT (RULE)` of each connection of `job` that its policy
    /// refused, once each.
    pub fn denials(&self, job: &str) -> Vec<String> {
        self.denials.of(job)
    }

    /// The proxy URL of a socket for the unit, in the form that `podnet pod
    /// -x` parses.
    pub fn proxy_url(socket: &Path) -> String {
        format!("http://unix:{}", socket.display())
    }

    /// False after [`Egress::kill`]: slots then gives no capacity and ends
    /// the running units (fail closed).
    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// End the module: each listener and each connection ends.
    pub fn kill(&self) {
        self.alive.store(false, Ordering::SeqCst);
        let jobs: Vec<String> = self.open.lock().expect("the listeners").keys().cloned().collect();
        for j in jobs {
            self.close(&j);
        }
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tls_tests;
