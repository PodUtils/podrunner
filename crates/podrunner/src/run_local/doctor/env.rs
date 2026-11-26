//! The rows of `doctor --run` of the environment of a job: the proxy and
//! the CA files of the user, as `run` gives them to a job (T-265, T-635).

use std::path::Path;

/// A variable of a proxy (also `NO_PROXY` and `NODE_USE_ENV_PROXY`).
pub(super) fn is_proxy(k: &str) -> bool {
    k.to_ascii_lowercase().ends_with("_proxy")
}

/// What a job takes of the proxy and the CA of the user `all` (T-265), and
/// the CA file that Node gets (T-642, T-695). `host`: a job on
/// `--runner-dir`, which gets `NO_PROXY` also with no proxy; a podvm unit
/// gets it only with a proxy (T-635).
pub(super) fn proxy_line(all: &[(String, String)], host: bool) -> String {
    let vars: Vec<(String, String)> = all
        .iter()
        .filter(|(k, v)| !v.is_empty() && k != "PATH" && podrunner_local::exec::PASSED.contains(&k.as_str()))
        .cloned()
        .collect();
    let names: Vec<&str> = vars.iter().map(|(k, _)| k.as_str()).collect();
    // The entries of NO_PROXY that `run` drops and names.
    let dropped = match host || names.iter().any(|k| is_proxy(k)) {
        true => podrunner_local::exec::dropped_no_proxy(&all.iter().cloned().collect()),
        false => Vec::new(),
    };
    let dropped: String = dropped
        .iter()
        .map(|e| format!("; NO_PROXY: the job does not get the entry {e:?} (no host: the official runner stops on it)"))
        .collect();
    if names.is_empty() {
        return format!("a job takes no proxy and no CA file of the environment (direct egress){dropped}");
    }
    let mut line = format!("a job takes {} of the environment", names.join(", "));
    // The rule of `run` itself, so the row cannot differ from the job.
    let mut job: std::collections::BTreeMap<String, String> = vars.iter().cloned().collect();
    let own = job.contains_key("NODE_EXTRA_CA_CERTS");
    podrunner_local::exec::node_ca(&mut job);
    match (job.get("NODE_EXTRA_CA_CERTS"), job.contains_key("SSL_CERT_FILE")) {
        (Some(node), true) if own => {
            line.push_str(&format!("; Node gets {node} (NODE_EXTRA_CA_CERTS of the user wins)"))
        }
        (Some(file), true) => line.push_str(&format!("; Node gets {file} (NODE_EXTRA_CA_CERTS from SSL_CERT_FILE)")),
        _ => {}
    }
    line + &dropped
}

/// Each CA file that a job takes from the environment `vars` can be read
/// (T-635): `run` copies each one into a podvm unit and stops on a file
/// that it cannot read, and a job on a runner directory reads them itself.
/// A podvm unit does not take `SSL_CERT_DIR`.
pub(super) fn ca_files(vars: &[(String, String)], unit: bool) -> Result<String, String> {
    let mut named = Vec::new();
    for (k, v) in vars {
        let passed = podrunner_local::exec::PASSED.contains(&k.as_str());
        if v.is_empty() || !passed || k == "PATH" || is_proxy(k) || (unit && k == "SSL_CERT_DIR") {
            continue;
        }
        let (ok, what) = match k == "SSL_CERT_DIR" {
            true => (std::fs::read_dir(v).is_ok(), "directory"),
            false => (Path::new(v).is_file() && std::fs::File::open(v).is_ok(), "file"),
        };
        if !ok {
            return Err(format!(
                "{k}={v}: not a {what} that this user can read; the run fails on it; fix: unset {k}, or name a {what} that is there"
            ));
        }
        named.push(k.as_str());
    }
    named.sort_unstable();
    Ok(match named.is_empty() {
        true => "no CA file of the environment".into(),
        false => format!("{} can be read", named.join(", ")),
    })
}
