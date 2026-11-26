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
//! places = ["header", "basic"]           # the default
//! ```
//!
//! A host that gives back a changed form of a value (in a part of it, or
//! in an encoding that the broker does not know) is not scrubbed: keep
//! `hosts` narrow, and `places` at `header` and `basic` unless a host needs
//! more.

use std::collections::BTreeMap;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
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
    /// The sources whose offers may name the secret. A credential of
    /// `default_policy` that names it holds for a job of these sources
    /// only; a job of another source is refused.
    pub sources: Vec<String>,
    /// The places of a request where the broker may put the value (`in` of
    /// a credential: `header`, `basic`, `query`, `body`). Default: `header`
    /// and `basic`. A credential with a wider `in` is refused: a value in a
    /// body or a query can reach a host that keeps it and shows it again.
    #[serde(default = "default_places")]
    pub places: Vec<String>,
}

/// The places of `pod-broker` for a credential with no `in`.
const DEFAULT_PLACES: [&str; 2] = ["header", "basic"];

fn default_places() -> Vec<String> {
    DEFAULT_PLACES.map(String::from).to_vec()
}

/// The name of the CA bundle of the broker among the credential files of a
/// unit.
pub const CA_FILE: &str = "podrunner-broker-ca.pem";

/// The name of the trust store of that bundle for a JVM (T-577).
pub const STORE_FILE: &str = "podrunner-broker-ca.p12";

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
        if s.places.is_empty() || s.places.iter().any(|p| !matches!(p.as_str(), "header" | "basic" | "query" | "body"))
        {
            return Err(format!("{at}: places takes header, basic, query and body"));
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
/// its hosts is a host of that secret, its places (`in`) are places of that
/// secret, no placeholder holds another, and (for a job of the source
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
        let places: Vec<&str> = match c.get("in").and_then(Value::as_array) {
            Some(a) if !a.is_empty() => a.iter().map(|p| p.as_str().unwrap_or("?")).collect(),
            _ => DEFAULT_PLACES.to_vec(),
        };
        if let Some(p) = places.iter().find(|p| !s.places.iter().any(|x| x == *p)) {
            return Err(format!("the secret {secret} does not go in the place {p}"));
        }
    }
    // The broker refuses two placeholders where one holds the other; the
    // refusal comes here, before a unit starts.
    let placeholders: Vec<(&str, &str)> = credentials(policy)
        .iter()
        .filter_map(|c| Some((c.get("placeholder")?.as_str()?, c.get("secret")?.as_str()?)))
        .collect();
    for (i, (a, sa)) in placeholders.iter().enumerate() {
        if let Some((_, sb)) = placeholders[i + 1..].iter().find(|(b, _)| a.contains(b) || b.contains(a)) {
            return Err(format!("the placeholders of the secrets {sa} and {sb} hold each other"));
        }
    }
    Ok(())
}

/// Check the credentials of a job of the source `source`: those of its
/// offer, then those of its policy after the ceiling (`default_policy`
/// adds its own), each for that source.
pub fn check_job(offer: &Map<String, Value>, egress: &crate::policy::EgressConfig, source: &str) -> Result<(), String> {
    check_credentials(offer, &egress.secrets, Some(source))?;
    let job = crate::policy::of_job(Some(offer), egress.default_policy.as_ref());
    let job: Map<String, Value> = serde_json::from_str(&job).map_err(|_| "the policy is not JSON".to_string())?;
    check_credentials(&job, &egress.secrets, Some(source))
}

/// Whether a policy names a credential: a unit of its job must not run as
/// the user of podrunner, who can read the files of the secrets.
pub fn has_credentials(policy: &Map<String, Value>) -> bool {
    !credentials(policy).is_empty()
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
        out.push((name.to_string(), read_value(&s.file).map_err(|why| format!("the secret {name}: {why}"))?));
    }
    Ok(out)
}

/// The value in `file`: one open with no symbolic link, then the checks of
/// that open file (a regular file of the effective user, mode 0600 or
/// 0400). The bytes stay in one buffer that becomes the `Secret`, which
/// wipes them at its drop; a buffer that is not UTF-8 is wiped here.
fn read_value(file: &std::path::Path) -> Result<Secret, String> {
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(file)
        .map_err(|e| format!("its file: {}", e.kind()))?;
    let meta = f.metadata().map_err(|e| format!("its file: {}", e.kind()))?;
    // SAFETY: geteuid has no precondition and cannot fail.
    let euid = unsafe { libc::geteuid() };
    if !meta.is_file() || meta.uid() != euid {
        return Err("its file is not a regular file of the user of podrunner".into());
    }
    if !matches!(meta.mode() & 0o777, 0o600 | 0o400) {
        return Err("its file is open to the group or to others: set mode 0600".into());
    }
    let mut bytes = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0).saturating_add(1).min(1 << 20));
    let read = f.by_ref().take(1 << 20).read_to_end(&mut bytes);
    if let Err(e) = read {
        wipe(bytes);
        return Err(format!("its file: {}", e.kind()));
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    if bytes.is_empty() {
        return Err("its file is empty".into());
    }
    match String::from_utf8(bytes) {
        Ok(text) => Ok(Secret::new(text)),
        Err(e) => {
            wipe(e.into_bytes());
            Err("its file is not UTF-8 text".into())
        }
    }
}

/// Write zeros over `bytes` before their drop; a volatile write is not
/// taken away as a dead store.
fn wipe(mut bytes: Vec<u8>) {
    for b in bytes.iter_mut() {
        // SAFETY: `b` is a valid, aligned and unique reference.
        unsafe { std::ptr::write_volatile(b, 0) };
    }
}

/// What a unit gets from its egress module: the count of flagged rules,
/// and the trust of its broker.
pub(crate) struct ForUnit {
    pub flagged: usize,
    pub ca: Option<UnitTrust>,
}

/// The trust of the broker of a unit whose broker ends TLS.
pub(crate) struct UnitTrust {
    /// The credential files: the bundle, then the trust store of a JVM.
    pub files: Vec<CredentialFile>,
    /// The variables of the unit: each of `ca::env` names the bundle, and
    /// `JAVA_TOOL_OPTIONS` names the store.
    pub env: Vec<(String, String)>,
    /// The bundle (PEM) and the variables that name it, for each container
    /// of the job (T-578).
    pub bundle: (String, Vec<String>),
    /// The trust store (PKCS #12) that the egress made, for the hooks of
    /// each container (T-693): they write it as it is.
    pub store: Option<Vec<u8>>,
}

/// Open the egress of `unit` of the source `source` with `policy` and the
/// values of its secrets. A unit whose broker ends its TLS gets the CA
/// bundle in `cred_dir`, where it sees its credential files; a kind that
/// does not say where is refused.
pub(crate) fn open(
    egress: &dyn EgressPort,
    unit: (&str, &str),
    policy: &str,
    store: &BTreeMap<String, SecretConfig>,
    socket: &std::path::Path,
    cred_dir: Option<&str>,
) -> Result<ForUnit, String> {
    let (unit, source) = unit;
    // Placement checked it; a check here keeps a credential of another
    // source out of each unit also if a path to the start skips placement.
    let p: Map<String, Value> = serde_json::from_str(policy).map_err(|_| "the policy is not JSON".to_string())?;
    check_credentials(&p, store, Some(source))?;
    let secrets = values(policy, store)?;
    let Opened { flagged, ca_bundle, ca_vars, trust_store } = egress.open_with(unit, policy, secrets, socket)?;
    let Some(bundle) = ca_bundle else { return Ok(ForUnit { flagged, ca: None }) };
    let Some(dir) = cred_dir else {
        egress.close(unit);
        return Err("the unit kind does not say where its unit sees its credential files: no CA bundle".into());
    };
    let dir = dir.trim_end_matches('/');
    let path = format!("{dir}/{CA_FILE}");
    let mut env: Vec<(String, String)> = ca_vars.iter().map(|k| (k.clone(), path.clone())).collect();
    let pem = (bundle.expose().to_string(), ca_vars);
    let mut files = vec![CredentialFile { name: CA_FILE.into(), content: bundle }];
    if let Some((store, options)) = &trust_store {
        // A path that a JVM cannot take (a space): the other clients still
        // trust the bundle.
        match options(std::path::Path::new(&format!("{dir}/{STORE_FILE}")), None) {
            Ok(value) => {
                files.push(podrunner_schema::binary_file(STORE_FILE, store));
                env.push(("JAVA_TOOL_OPTIONS".into(), value));
            }
            Err(why) => eprintln!("podrunner: {unit}: no trust store for a JVM: {why}"),
        }
    }
    Ok(ForUnit { flagged, ca: Some(UnitTrust { files, env, bundle: pem, store: trust_store.map(|(store, _)| store) }) })
}
