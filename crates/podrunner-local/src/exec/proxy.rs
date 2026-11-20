//! The proxy and the CA files of the caller in a unit with the network of
//! the host (T-284): on a host whose only way out is a proxy that
//! intercepts TLS, a job needs both. Each CA file goes into the unit as a
//! credential file, with no volume (a volume moves a podvm unit off the
//! namespace rung, T-261).

use std::collections::BTreeMap;

use podrunner_schema::{CredentialFile, Secret};

use super::host::PASSED;

/// The variables that name a CA file: the ten of `pod_broker::ca::env`,
/// `pod_tls::BUNDLE_VARS` and then `pod_broker::ca::CLIENT_VARS` (Deno,
/// Hex, the Google Cloud CLI, cargo). Each is in `PASSED` (a test).
const CA_VARS: [&str; 10] = {
    let (b, c) = (pod_tls::BUNDLE_VARS, pod_broker::ca::CLIENT_VARS);
    [b[0], b[1], b[2], b[3], b[4], b[5], c[0], c[1], c[2], c[3]]
};

/// The variables and the CA files of a unit.
pub(crate) type UnitEnv = (Vec<(String, String)>, Vec<CredentialFile>);

/// The environment of a unit and its CA files, from the variables `vars` of
/// the caller; `dir` is where the unit sees its credential files. A `held`
/// unit takes the CA files only: its hold gives it its way out, and the
/// proxy of the host behind the hold can intercept TLS (T-320).
pub(crate) fn unit_env(dir: &str, vars: impl Iterator<Item = (String, String)>, held: bool) -> Result<UnitEnv, String> {
    let (env, files) = all_env(dir, vars)?;
    Ok(match held {
        true => (env.into_iter().filter(|(k, _)| CA_VARS.contains(&k.as_str())).collect(), files),
        false => (env, files),
    })
}

fn all_env(dir: &str, vars: impl Iterator<Item = (String, String)>) -> Result<UnitEnv, String> {
    let vars: BTreeMap<String, String> = vars.filter(|(_, v)| !v.is_empty()).collect();
    let mut env = Vec::new();
    let mut files: Vec<CredentialFile> = Vec::new();
    let mut by_path: BTreeMap<String, String> = BTreeMap::new();
    for k in PASSED.iter().filter(|k| **k != "PATH" && **k != "SSL_CERT_DIR") {
        let Some(v) = vars.get(*k) else { continue };
        if !CA_VARS.contains(k) {
            env.push((k.to_string(), v.clone()));
            continue;
        }
        let name = match by_path.get(v) {
            Some(n) => n.clone(),
            None => {
                let text = std::fs::read_to_string(v).map_err(|e| format!("{k}={v}: {e}"))?;
                let n = format!("podrunner-ca-{}.crt", by_path.len());
                files.push(CredentialFile { name: n.clone(), content: Secret::new(text) });
                by_path.insert(v.clone(), n.clone());
                n
            }
        };
        env.push((k.to_string(), format!("{dir}/{name}")));
    }
    let view: Vec<(&str, String)> = env.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    if let Some((k, v)) = super::host::node_proxy(&view) {
        env.push((k.to_string(), v));
    }
    if env.iter().any(|(k, _)| k.to_ascii_lowercase().ends_with("_proxy") && k != "NODE_USE_ENV_PROXY") {
        let no = super::host::no_proxy(&vars);
        env.push(("NO_PROXY".into(), no.clone()));
        env.push(("no_proxy".into(), no));
    }
    Ok((env, files))
}

/// The variables of a unit that do not come from the caller. A podvm unit
/// sets `UV_USE_IO_URING=0`: podvm maps the paths of libc calls only, and
/// the io_uring of libuv passes no libc call (T-262, PodUtils/podvm#439).
/// A held unit names the port of its broker on the loopback: the driver
/// lets that port through the hold and refuses the rest of the loopback
/// (T-319).
pub(crate) fn unit_extra(env: &mut std::collections::BTreeMap<String, String>, guest: bool, held_broker: Option<&str>) {
    if !guest {
        env.insert("UV_USE_IO_URING".into(), "0".into());
    }
    let port =
        held_broker.and_then(|b| b.trim_end_matches('/').rsplit(':').next()).filter(|p| p.parse::<u16>().is_ok());
    if let Some(p) = port {
        env.insert(podrunner_unit::LOOPBACK_ALLOW.into(), p.into());
    }
}

/// The hosts that the egress hold refused for unit `id` of `job`, into its
/// log; for a failed job also on stderr, with the fix (T-328). A host that
/// the policy allows and that the module found no way to (the rules
/// `resolve` and `upstream`) is named as such: an allow does not fix it
/// (T-355).
pub(crate) fn denials(log: &mut String, job: &str, id: &str, denied: &[String], failed: bool) {
    for d in denied {
        log.push_str(&format!("\npodrunner: the egress policy of {id} denied {d}"));
    }
    let (refused, no_way) = split(denied);
    if failed && !refused.is_empty() {
        eprintln!(
            "podrunner: {job}: the egress hold refused {}; if the job needs one, allow it: --allow HOST, or `allow` in run.toml",
            refused.join(", ")
        );
    }
    if failed && !no_way.is_empty() {
        eprintln!(
            "podrunner: {job}: the egress module found no way to {}; an allow does not fix this: see `podrunner doctor`",
            no_way.join(", ")
        );
    }
}

/// The hosts that the policy refused, and the hosts with no way to them
/// (the rules `resolve` and `upstream`), each with its cause.
fn split(denied: &[String]) -> (Vec<String>, Vec<String>) {
    let (mut refused, mut no_way): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for d in denied {
        let Some((host, rest)) = d.split_once(':') else { continue };
        if host.is_empty() || host == "null" {
            continue;
        }
        let rule = rest.split_once(" (").map_or("", |(_, r)| r.trim_end_matches(')'));
        let item = match rule {
            "resolve" => format!("{host} (no address: the resolver of this host)"),
            "upstream" => format!("{host} (the proxy of this host)"),
            _ => host.to_string(),
        };
        let list = if matches!(rule, "resolve" | "upstream") { &mut no_way } else { &mut refused };
        if !list.contains(&item) {
            list.push(item);
        }
    }
    (refused, no_way)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_gets_the_proxy_and_each_ca_file_once() {
        let ca = std::env::temp_dir().join(format!("podrunner-proxy-ca-{}.crt", std::process::id()));
        std::fs::write(&ca, "-----BEGIN CERTIFICATE-----\nmade-up\n").unwrap();
        let p = ca.display().to_string();
        let vars = [
            ("HTTPS_PROXY", "http://127.0.0.1:3128"),
            ("SSL_CERT_FILE", p.as_str()),
            ("NODE_EXTRA_CA_CERTS", p.as_str()),
            ("PATH", "/x"),
            ("NO_PROXY", "::,intra.example"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let (env, files) = unit_env("/podrunner/.runner", vars.clone().into_iter(), false).unwrap();
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("HTTPS_PROXY"), Some("http://127.0.0.1:3128"));
        assert_eq!(get("SSL_CERT_FILE"), Some("/podrunner/.runner/podrunner-ca-0.crt"));
        assert_eq!(get("NODE_EXTRA_CA_CERTS"), Some("/podrunner/.runner/podrunner-ca-0.crt"));
        assert_eq!(get("PATH"), None, "the unit keeps the PATH of its image");
        assert_eq!(get("NO_PROXY"), Some("127.0.0.1,localhost,::1,intra.example"));
        assert_eq!(get("NODE_USE_ENV_PROXY"), Some("1"), "fetch of Node uses the proxy (T-299)");
        assert_eq!(files.len(), 1, "one file for one path");
        let (held, _) = unit_env("/podrunner/.runner", vars.into_iter(), true).unwrap();
        let held = |k: &str| held.iter().any(|(n, _)| n == k);
        assert!(
            held("NODE_EXTRA_CA_CERTS") && !held("HTTPS_PROXY") && !held("NO_PROXY"),
            "a held unit: the CA only (T-320)"
        );
        let (env, files) = unit_env("/d", std::iter::empty(), false).unwrap();
        assert!(env.is_empty() && files.is_empty(), "no proxy, nothing");
        let _ = std::fs::remove_file(&ca);
    }

    /// T-576: each variable of `pod_broker::ca::env` names a CA file that a
    /// unit takes from the caller.
    #[test]
    fn each_variable_of_the_bundle_of_the_broker_is_passed() {
        let path = std::path::Path::new("/ca.pem");
        let vars: Vec<&str> = pod_broker::ca::env(path).into_iter().map(|(k, _)| k).collect();
        assert_eq!(vars, CA_VARS);
        assert!(CA_VARS.iter().all(|k| PASSED.contains(k)), "{CA_VARS:?}");
        let ca = std::env::temp_dir().join(format!("podrunner-proxy-cargo-{}.crt", std::process::id()));
        std::fs::write(&ca, "-----BEGIN CERTIFICATE-----\nmade-up\n").unwrap();
        let p = ca.display().to_string();
        let given = [("CARGO_HTTP_CAINFO", p.clone()), ("DENO_CERT", p)].map(|(k, v)| (k.to_string(), v));
        let (env, files) = unit_env("/d", given.into_iter(), true).unwrap();
        assert_eq!(files.len(), 1, "one file for one path");
        assert!(env.iter().any(|(k, v)| k == "CARGO_HTTP_CAINFO" && v == "/d/podrunner-ca-0.crt"), "{env:?}");
        assert!(env.iter().any(|(k, v)| k == "DENO_CERT" && v == "/d/podrunner-ca-0.crt"), "{env:?}");
        let _ = std::fs::remove_file(&ca);
    }

    /// T-328: the log names each refused host; the hosts are a list.
    #[test]
    fn the_refused_hosts_go_into_the_log() {
        let mut log = String::new();
        let denied = [":0 (usage)".to_string(), "nodejs.org:443 (policy:no-rule)".to_string()];
        denials(&mut log, "j.0", "u-1", &denied, true);
        assert!(log.contains("u-1 denied nodejs.org:443"), "{log}");
    }

    /// T-355: a host with no route is not a host to allow.
    #[test]
    fn a_failed_route_is_not_a_refusal() {
        let denied = ["github.com:443 (resolve)".to_string(), "example.com:443 (policy:no-rule)".to_string()];
        let (refused, no_way) = split(&denied);
        assert_eq!(refused, ["example.com"]);
        assert_eq!(no_way, ["github.com (no address: the resolver of this host)"]);
        let mut log = String::new();
        denials(&mut log, "j.0", "u-1", &denied, false);
        assert!(log.contains("github.com:443 (resolve)"), "{log}");
    }

    /// T-319: a held unit names the port of its broker.
    #[test]
    fn a_held_unit_names_the_port_of_its_broker() {
        let mut env = BTreeMap::new();
        unit_extra(&mut env, false, Some("http://127.0.0.1:41234/"));
        assert_eq!(env.get(podrunner_unit::LOOPBACK_ALLOW).map(String::as_str), Some("41234"));
        let mut env = BTreeMap::new();
        unit_extra(&mut env, true, None);
        assert!(env.is_empty());
    }
}
