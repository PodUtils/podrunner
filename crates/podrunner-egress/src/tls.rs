//! The TLS end of the egress module (T-563, T-524): the broker reads the
//! tunnels to the hosts of a credential and to the hosts whose rules have
//! a path. The unit holds a placeholder; the far end gets the value of the
//! secret. The broker makes a CA for each such job, and the job trusts the
//! bundle of that CA and the roots of the host.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::EgressError;

/// The trust of the host for the TLS of the broker to a far end, and the
/// roots that a job trusts beside the CA of its job.
#[derive(Clone)]
pub struct HostTls {
    /// The client configuration of the broker to a far end.
    pub trust: Arc<rustls::ClientConfig>,
    /// The roots of the bundle of a job (PEM). None: the host has no CA
    /// file, and a job that needs a TLS end is refused.
    pub roots_pem: Option<Vec<u8>>,
}

impl HostTls {
    /// The trust of this host: the CA file that the environment names (an
    /// intercepting proxy of the host, T-284) beside the default roots of
    /// `pod-tls`. The roots of a job are that CA file and the first
    /// readable system bundle.
    pub fn of_host() -> Result<HostTls, EgressError> {
        let (env, _) = pod_tls::env_bundle(|v| std::env::var(v).ok());
        let env = env.map(|b| b.path);
        let mut settings = pod_tls::Settings::new(pod_tls::Words::new("podrunner"));
        settings.extra_bundle = env.clone();
        let context = pod_tls::Context::new(provider(), settings);
        let trust = context
            .config_for(&pod_tls::Trust::Default)
            .map_err(|e| EgressError(format!("the trust of the TLS end: {e}")))?;
        let system: Vec<PathBuf> = pod_tls::SYSTEM_BUNDLES.iter().map(PathBuf::from).collect();
        Ok(HostTls { trust, roots_pem: roots_pem(env.as_deref(), &system) })
    }
}

/// The variables that name the CA bundle of a job in its unit: the ten of
/// `ca::env` of `pod-broker`.
pub fn ca_vars() -> Vec<String> {
    pod_broker::ca::env(Path::new("")).into_iter().map(|(k, _)| k.to_string()).collect()
}

/// The provider of the TLS of podrunner.
pub fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// The roots of the bundle of a job: the CA file `env` of the environment,
/// then the first file of `system` that holds a certificate. None when no
/// file holds one.
pub fn roots_pem(env: Option<&Path>, system: &[PathBuf]) -> Option<Vec<u8>> {
    let read =
        |p: &Path| std::fs::read(p).ok().filter(|b| pod_tls::bundle::pem_certificates(b).is_ok_and(|c| !c.is_empty()));
    let mut out = Vec::new();
    if let Some(b) = env.and_then(read) {
        out.extend_from_slice(&b);
        if !out.ends_with(b"\n") {
            out.push(b'\n');
        }
    }
    if let Some(b) = system.iter().find_map(|p| read(p)) {
        out.extend_from_slice(&b);
    }
    (!out.is_empty()).then_some(out)
}

/// The trust store of the bundle of a job for a JVM (T-577): the CA of
/// the job and `roots` in PKCS #12.
pub(crate) fn trust_store(job: &str, ca_pem: &str, roots: Option<&[u8]>) -> Result<Vec<u8>, EgressError> {
    let roots = roots.ok_or_else(|| EgressError(format!("{job}: no roots for its trust store")))?;
    pod_broker::ca::trust_store(ca_pem, roots).map_err(|e| EgressError(format!("the trust store of {job}: {e}")))
}

/// The value of `JAVA_TOOL_OPTIONS` that names the trust store at `store`
/// in a unit, before the value `job` of the job (`ca::java_options`).
pub fn java_options(store: &Path, job: Option<&str>) -> Result<String, String> {
    pod_broker::ca::java_options(store, job)
}

/// The bundle of a job: the PEM of its CA, then `roots`.
pub(crate) fn bundle(job: &str, ca_pem: &str, roots: Option<&[u8]>) -> Result<String, EgressError> {
    let Some(roots) = roots else {
        return Err(EgressError(format!(
            "{job} needs the CA of its broker, and this host has no CA file for the roots of its bundle"
        )));
    };
    pod_broker::ca::bundle(ca_pem, roots).map_err(|e| EgressError(format!("the CA bundle of {job}: {e}")))
}
