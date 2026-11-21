//! The secrets of the host (T-563, githome's G-35): a credential of a
//! policy names a secret, never its value. The unit holds the placeholder
//! of the credential; the broker gives the value to the hosts of the
//! credential only. The operator keeps each value in a file and names the
//! hosts that may get it and the sources whose offers may name it:
//!
//! ```toml
//! [egress.secrets.github]
//! file = "/etc/podrunner/secrets/github"   # mode 0600 or 0400
//! hosts = ["api.github.com", "*.githubusercontent.com"]
//! sources = ["githome"]
//! ```

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use podrunner_schema::{CredentialFile, Secret};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::egress_port::{EgressPort, Opened};

/// A secret of the host.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretConfig {
    /// The file of the value (mode 0600 or 0400). One end of line at the
    /// end is not part of the value. slots reads it at the start of each
    /// unit, so a new value needs no restart.
    pub file: PathBuf,
    /// The hosts that may get the value (as the `host` of a rule): each
    /// host of a credential that names the secret is one of them, or under
    /// a `*.NAME` of them.
    pub hosts: Vec<String>,
    /// The sources whose offers may name the secret.
    pub sources: Vec<String>,
}

/// The name of the CA bundle of the broker among the credential files of a
/// unit.
pub const CA_FILE: &str = "podrunner-broker-ca.pem";

/// The check of the secrets of `[egress]`.
pub(crate) fn check_store(store: &BTreeMap<String, SecretConfig>) -> Result<(), String> {
    let fine = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-');
    for (name, s) in store {
        let at = format!("[egress.secrets.{name}]");
        if name.is_empty() || name.len() > 128 || !name.chars().all(fine) {
            return Err(format!("{at}: a name has 1 to 128 of A-Z a-z 0-9 . _ : -"));
        }
        if !s.file.is_absolute() {
            return Err(format!("{at}: file must be an absolute path"));
        }
        if s.hosts.is_empty() || s.hosts.iter().any(|h| host(h).is_empty() || host(h) == "*") {
            return Err(format!("{at}: hosts names one host or more"));
        }
        if s.sources.is_empty() {
            return Err(format!("{at}: sources names one source or more"));
        }
    }
    Ok(())
}

/// A host in lowercase with no dot at its end.
fn host(h: &str) -> String {
    h.strip_suffix('.').unwrap_or(h).to_ascii_lowercase()
}

/// The credentials of a policy.
fn credentials(p: &Map<String, Value>) -> &[Value] {
    p.get("credentials").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default()
}

/// Check each credential of `policy`: it names a secret of `store`, each of
/// its hosts is a host of that secret, and (for an offer of the source
/// `source`) the secret is for that source. The words name a secret and a
/// host, never a value.
pub fn check_credentials(
    policy: &Map<String, Value>,
    store: &BTreeMap<String, SecretConfig>,
    source: Option<&str>,
) -> Result<(), String> {
    if policy.get("credentials").is_some_and(|c| !c.is_array()) {
        return Err("credentials is a list".into());
    }
    for (n, c) in credentials(policy).iter().enumerate() {
        let secret = c.get("secret").and_then(Value::as_str).ok_or(format!("credential {n} names no secret"))?;
        let hosts: Vec<&str> =
            c.get("hosts").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
        if hosts.is_empty() {
            return Err(format!("credential {n} names no host"));
        }
        let Some(s) = store.get(secret) else {
            return Err(format!("credential {n} names the secret {secret}, which this host does not hold"));
        };
        if let Some(source) = source.filter(|src| !s.sources.iter().any(|x| x == src)) {
            return Err(format!("the secret {secret} is not for the source {source}"));
        }
        if let Some(h) = hosts.iter().find(|h| !s.hosts.iter().any(|x| crate::policy::covers(&host(x), &host(h)))) {
            return Err(format!("the secret {secret} does not go to {h}"));
        }
    }
    Ok(())
}

/// The values of the secrets that the credentials of `policy` (JSON) name,
/// read from the files of `store`. A file open to the group or to others
/// is refused.
pub fn values(policy: &str, store: &BTreeMap<String, SecretConfig>) -> Result<Vec<(String, Secret)>, String> {
    let p: Map<String, Value> = serde_json::from_str(policy).map_err(|_| "the policy is not JSON".to_string())?;
    let mut out: Vec<(String, Secret)> = Vec::new();
    for c in credentials(&p) {
        let Some(name) = c.get("secret").and_then(Value::as_str) else { continue };
        if out.iter().any(|(n, _)| n == name) {
            continue;
        }
        let s = store.get(name).ok_or(format!("the secret {name}: this host does not hold it"))?;
        let meta = std::fs::metadata(&s.file).map_err(|e| format!("the secret {name}: its file: {}", e.kind()))?;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(format!("the secret {name}: its file is open to the group or to others: set mode 0600"));
        }
        let text =
            std::fs::read_to_string(&s.file).map_err(|e| format!("the secret {name}: its file: {}", e.kind()))?;
        let value = text.strip_suffix('\n').map(|t| t.strip_suffix('\r').unwrap_or(t)).unwrap_or(&text);
        if value.is_empty() {
            return Err(format!("the secret {name}: its file is empty"));
        }
        out.push((name.to_string(), Secret::new(value)));
    }
    Ok(out)
}

/// What a unit gets from its egress module: the count of flagged rules,
/// and the CA bundle of its broker as a credential file with the variables
/// that name it.
pub(crate) struct ForUnit {
    pub flagged: usize,
    pub ca: Option<(CredentialFile, Vec<(String, String)>)>,
}

/// Open the egress of `unit` with `policy` and the values of its secrets.
/// A unit whose broker ends its TLS gets the CA bundle in `cred_dir`, where
/// it sees its credential files; a kind that does not say where is refused.
pub(crate) fn open(
    egress: &dyn EgressPort,
    unit: &str,
    policy: &str,
    store: &BTreeMap<String, SecretConfig>,
    socket: &std::path::Path,
    cred_dir: Option<&str>,
) -> Result<ForUnit, String> {
    let secrets = values(policy, store)?;
    let Opened { flagged, ca_bundle, ca_vars } = egress.open_with(unit, policy, secrets, socket)?;
    let Some(bundle) = ca_bundle else { return Ok(ForUnit { flagged, ca: None }) };
    let Some(dir) = cred_dir else {
        egress.close(unit);
        return Err("the unit kind does not say where its unit sees its credential files: no CA bundle".into());
    };
    let path = format!("{}/{CA_FILE}", dir.trim_end_matches('/'));
    let env = ca_vars.into_iter().map(|k| (k, path.clone())).collect();
    Ok(ForUnit { flagged, ca: Some((CredentialFile { name: CA_FILE.into(), content: bundle }, env)) })
}
