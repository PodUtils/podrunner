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

/// The variables and the CA files of a unit, and the bundle of the CA
/// files for each container of the job (T-596).
pub(crate) type UnitEnv = (Vec<(String, String)>, Vec<CredentialFile>, Option<String>);

/// The name of the bundle of the CA files in each container of the job:
/// `/__podrunner/podrunner-ca.pem` (T-596).
pub(crate) const CONTAINER_CA: &str = "podrunner-ca.pem";

/// The trust store of the CA files of the caller for a JVM (T-577): a JVM
/// reads none of `CA_VARS`.
pub(crate) const STORE_FILE: &str = "podrunner-ca.p12";

/// The variable of the options of each JVM: it names the trust store.
const JAVA_OPTIONS: &str = "JAVA_TOOL_OPTIONS";

/// The environment of a unit and its CA files, from the variables `vars` of
/// the caller; `dir` is where the unit sees its credential files. A `held`
/// unit takes the CA files only: its hold gives it its way out, and the
/// proxy of the host behind the hold can intercept TLS (T-320).
pub(crate) fn unit_env(dir: &str, vars: impl Iterator<Item = (String, String)>, held: bool) -> Result<UnitEnv, String> {
    let (env, files, pem) = all_env(dir, vars)?;
    Ok(match held {
        true => {
            (env.into_iter().filter(|(k, _)| CA_VARS.contains(&k.as_str()) || k == JAVA_OPTIONS).collect(), files, pem)
        }
        false => (env, files, pem),
    })
}

/// The variables that name the bundle in each container of the job: the
/// ten of `pod_broker::ca::env`.
pub(crate) fn container_vars() -> Vec<String> {
    CA_VARS.iter().map(|v| v.to_string()).collect()
}

fn all_env(dir: &str, vars: impl Iterator<Item = (String, String)>) -> Result<UnitEnv, String> {
    let vars: BTreeMap<String, String> = vars.filter(|(_, v)| !v.is_empty()).collect();
    let mut env = Vec::new();
    let mut files: Vec<CredentialFile> = Vec::new();
    let mut by_path: BTreeMap<String, String> = BTreeMap::new();
    // Each CA file once (a link and its target too), for the trust store
    // of a JVM and the bundle of the containers.
    let mut pem = String::new();
    let mut in_pem: Vec<std::path::PathBuf> = Vec::new();
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
                // The runner still gets the file as it is.
                add_to_bundle(&mut pem, &mut in_pem, (k, v), &text, "no container gets it");
                let n = format!("podrunner-ca-{}.crt", by_path.len());
                files.push(CredentialFile { name: n.clone(), content: Secret::new(text) });
                by_path.insert(v.clone(), n.clone());
                n
            }
        };
        env.push((k.to_string(), format!("{dir}/{name}")));
    }
    if !pem.is_empty() {
        match java_trust(dir, &pem) {
            Ok((store, options)) => {
                files.push(store);
                env.push((JAVA_OPTIONS.into(), options));
            }
            // The other clients of the job still trust the CA files.
            Err(why) => eprintln!("{NO_STORE}: {why}"),
        }
    }
    let view: Vec<(&str, String)> = env.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    if let Some((k, v)) = super::host::node_proxy(&view) {
        env.push((k.to_string(), v));
    }
    if env.iter().any(|(k, _)| k.to_ascii_lowercase().ends_with("_proxy") && k != "NODE_USE_ENV_PROXY") {
        let (no, dropped) = super::noproxy::job_list(&vars);
        super::noproxy::name_dropped(&dropped);
        env.push(("NO_PROXY".into(), no.clone()));
        env.push(("no_proxy".into(), no));
    }
    Ok((env, files, (!pem.is_empty()).then_some(pem)))
}

/// The start of a line on stderr for a CA file that the trust store of a
/// JVM does not take (T-577).
const NO_STORE: &str = "podrunner: no trust store of the CA files for a JVM";

/// Add the CA file of the variable `k` (path `v`, text `text`) to `pem`,
/// once for each file (`seen`: the canonical path of each file in it). A
/// file with no certificate stays out, and one line ends with `left`.
fn add_to_bundle(pem: &mut String, seen: &mut Vec<std::path::PathBuf>, (k, v): (&str, &str), text: &str, left: &str) {
    let real = std::fs::canonicalize(v).unwrap_or_else(|_| v.into());
    match pod_tls::bundle::pem_certificates(text.as_bytes()) {
        _ if seen.contains(&real) => {}
        Ok(c) if !c.is_empty() => {
            pem.push_str(text);
            if !text.ends_with('\n') {
                pem.push('\n');
            }
            seen.push(real);
        }
        Ok(_) => eprintln!("{NO_STORE}: {k}={v}: the file holds no certificate; {left}"),
        Err(why) => eprintln!("{NO_STORE}: {k}={v}: {why}; {left}"),
    }
}

/// The trust store of the CA files of `env` for a JVM of a job on a
/// runner directory (T-598): written to `home` (the scratch home of the
/// job, which the next job removes), and `JAVA_TOOL_OPTIONS` that names
/// it. As in a unit, the value of the caller stays out. A CA file that
/// cannot be read, or a store that cannot be made, gives one line and no
/// store: the other clients of the job still trust the CA files.
pub(crate) fn host_trust(home: &std::path::Path, env: &mut Vec<(&'static str, String)>) {
    let (mut pem, mut seen) = (String::new(), Vec::new());
    for (k, v) in env.iter().filter(|(k, _)| CA_VARS.contains(k)) {
        match std::fs::read_to_string(v) {
            Ok(text) => add_to_bundle(&mut pem, &mut seen, (k, v), &text, "the JVM does not trust it"),
            Err(e) => eprintln!("{NO_STORE}: {k}={v}: {e}; the JVM does not trust it"),
        }
    }
    if pem.is_empty() {
        return;
    }
    let store = home.join(STORE_FILE);
    let made = pod_broker::ca::write_trust_store(&store, "", pem.as_bytes()).map_err(|e| e.to_string());
    match made.and_then(|()| pod_broker::ca::java_options(&store, None)) {
        Ok(options) => env.push((JAVA_OPTIONS, options)),
        Err(why) => {
            let _ = std::fs::remove_file(&store);
            eprintln!("{NO_STORE}: {why}");
        }
    }
}

/// The trust store of the CA files `pem` in the unit (`dir`), and the
/// value of `JAVA_TOOL_OPTIONS` that names it (T-577). The value of the
/// caller is not in `PASSED` and stays out: it can hold a password (for
/// example `-Dhttp.proxyPassword=`). A job sets its own value.
fn java_trust(dir: &str, pem: &str) -> Result<(CredentialFile, String), String> {
    // The CA files hold the CA of the proxy and the roots of the caller:
    // the store takes each certificate of them in order.
    let der = pod_broker::ca::trust_store("", pem.as_bytes())?;
    let options = pod_broker::ca::java_options(std::path::Path::new(&format!("{dir}/{STORE_FILE}")), None)?;
    Ok((podrunner_schema::binary_file(STORE_FILE, &der), options))
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
        let (env, files, _) = unit_env("/podrunner/.runner", vars.clone().into_iter(), false).unwrap();
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("HTTPS_PROXY"), Some("http://127.0.0.1:3128"));
        assert_eq!(get("SSL_CERT_FILE"), Some("/podrunner/.runner/podrunner-ca-0.crt"));
        assert_eq!(get("NODE_EXTRA_CA_CERTS"), Some("/podrunner/.runner/podrunner-ca-0.crt"));
        assert_eq!(get("PATH"), None, "the unit keeps the PATH of its image");
        assert_eq!(get("NO_PROXY"), Some("127.0.0.1,localhost,::1,intra.example"));
        assert_eq!(get("NODE_USE_ENV_PROXY"), Some("1"), "fetch of Node uses the proxy (T-299)");
        assert_eq!(files.len(), 1, "one file for one path");
        let (held, _, _) = unit_env("/podrunner/.runner", vars.into_iter(), true).unwrap();
        let held = |k: &str| held.iter().any(|(n, _)| n == k);
        assert!(
            held("NODE_EXTRA_CA_CERTS") && !held("HTTPS_PROXY") && !held("NO_PROXY"),
            "a held unit: the CA only (T-320)"
        );
        let (env, files, _) = unit_env("/d", std::iter::empty(), false).unwrap();
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
        let (env, files, _) = unit_env("/d", given.into_iter(), true).unwrap();
        assert_eq!(files.len(), 1, "one file for one path");
        assert!(env.iter().any(|(k, v)| k == "CARGO_HTTP_CAINFO" && v == "/d/podrunner-ca-0.crt"), "{env:?}");
        assert!(env.iter().any(|(k, v)| k == "DENO_CERT" && v == "/d/podrunner-ca-0.crt"), "{env:?}");
        let _ = std::fs::remove_file(&ca);
    }

    /// T-577: a caller with a CA file gives a unit the trust store of each
    /// CA file for a JVM, and `JAVA_TOOL_OPTIONS` that names it. The value
    /// of the caller is not in `PASSED`: it stays out of the unit (it can
    /// hold `-Dhttp.proxyPassword=`).
    #[test]
    fn a_caller_with_a_ca_file_gives_a_unit_a_trust_store() {
        use podrunner_schema::FileBytes;
        let dir = std::env::temp_dir().join(format!("podrunner-proxy-jvm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Made-up certificates: the store holds their bytes; no TLS reads them.
        let (one, two) = (dir.join("one.crt"), dir.join("two.crt"));
        std::fs::write(&one, "-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----\n").unwrap();
        std::fs::write(&two, "-----BEGIN CERTIFICATE-----\nBAUG\n-----END CERTIFICATE-----").unwrap();
        let (p1, p2) = (one.display().to_string(), two.display().to_string());
        let given = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        };
        let store = "/podrunner/.runner/podrunner-ca.p12";
        let want = format!("-Djavax.net.ssl.trustStore={store} -Djavax.net.ssl.trustStoreType=PKCS12");
        let secret = "-Dhttp.proxyPassword=made-up-value";
        let vars = given(&[("SSL_CERT_FILE", &p1), ("NODE_EXTRA_CA_CERTS", &p2), ("JAVA_TOOL_OPTIONS", secret)]);
        for held in [false, true] {
            let (env, files, _) = unit_env("/podrunner/.runner", vars.clone().into_iter(), held).unwrap();
            let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
            assert_eq!(get("JAVA_TOOL_OPTIONS"), Some(want.clone()), "held: {held}");
            assert!(env.iter().all(|(_, v)| !v.contains("made-up-value")), "held: {held}: {env:?}");
            let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
            assert_eq!(names, ["podrunner-ca-0.crt", "podrunner-ca-1.crt", STORE_FILE], "held: {held}");
            let der = files[2].bytes().unwrap();
            let has = |n: &[u8]| der.windows(n.len()).any(|w| w == n);
            assert!(has(&[0x04, 0x03, 1, 2, 3]) && has(&[0x04, 0x03, 4, 5, 6]), "each CA file: {der:02x?}");
        }
        // No value of the caller: the store alone.
        let (env, _, _) = unit_env("/d", given(&[("SSL_CERT_FILE", &p1)]).into_iter(), false).unwrap();
        let java = env.iter().find(|(k, _)| k == "JAVA_TOOL_OPTIONS").map(|(_, v)| v.as_str());
        let alone = "-Djavax.net.ssl.trustStore=/d/podrunner-ca.p12 -Djavax.net.ssl.trustStoreType=PKCS12";
        assert_eq!(java, Some(alone));
        // No CA file: no store, and the options of the caller stay out.
        let (env, files, _) = unit_env("/d", given(&[("JAVA_TOOL_OPTIONS", "-Xmx1g")]).into_iter(), false).unwrap();
        assert!(env.is_empty() && files.is_empty(), "{env:?}");
        // A CA file with no certificate, or a directory with a space (the JVM
        // splits the variable at each space): the CA files, and no store.
        let empty = dir.join("empty.crt");
        std::fs::write(&empty, "").unwrap();
        let e = empty.display().to_string();
        for (d, path) in [("/d", e.as_str()), ("/a b", p1.as_str())] {
            let (env, files, _) = unit_env(d, given(&[("SSL_CERT_FILE", path)]).into_iter(), false).unwrap();
            assert!(env.iter().all(|(k, _)| k != "JAVA_TOOL_OPTIONS"), "{d} {path}: {env:?}");
            assert_eq!(files.len(), 1, "{d} {path}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-598: a job on a runner directory gets the trust store of each CA
    /// file in its scratch home, and `JAVA_TOOL_OPTIONS` that names it; a
    /// file with no certificate, a missing file or a home with a space give
    /// no store.
    #[test]
    fn a_job_on_a_runner_directory_gets_a_trust_store() {
        let dir = std::env::temp_dir().join(format!("podrunner-proxy-host-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("home")).unwrap();
        std::fs::create_dir_all(dir.join("a b")).unwrap();
        let (one, two, link) = (dir.join("one.crt"), dir.join("two.crt"), dir.join("link.crt"));
        std::fs::write(&one, "-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----\n").unwrap();
        std::fs::write(&two, "-----BEGIN CERTIFICATE-----\nBAUG\n-----END CERTIFICATE-----").unwrap();
        std::os::unix::fs::symlink(&one, &link).unwrap();
        let p = |f: &std::path::Path| f.display().to_string();
        let env = |pairs: &[(&'static str, String)]| -> Vec<(&'static str, String)> {
            let mut e = vec![("PATH", "/bin".to_string())];
            e.extend(pairs.iter().cloned());
            e
        };
        let home = dir.join("home");
        let mut e = env(&[("SSL_CERT_FILE", p(&link)), ("CURL_CA_BUNDLE", p(&one)), ("GIT_SSL_CAINFO", p(&two))]);
        host_trust(&home, &mut e);
        let store = home.join(STORE_FILE);
        let want = format!("-Djavax.net.ssl.trustStore={} -Djavax.net.ssl.trustStoreType=PKCS12", store.display());
        assert_eq!(e.last(), Some(&("JAVA_TOOL_OPTIONS", want)), "{e:?}");
        let der = std::fs::read(&store).unwrap();
        let count = |n: &[u8]| der.windows(n.len()).filter(|w| *w == n).count();
        assert_eq!((count(&[0x04, 0x03, 1, 2, 3]), count(&[0x04, 0x03, 4, 5, 6])), (1, 1), "each file once");
        // No CA variable, a file with no certificate, a missing file: no store.
        let empty = dir.join("empty.crt");
        std::fs::write(&empty, "").unwrap();
        for pairs in [vec![], vec![("SSL_CERT_FILE", p(&empty))], vec![("SSL_CERT_FILE", p(&dir.join("absent")))]] {
            let home = dir.join("none");
            std::fs::create_dir_all(&home).unwrap();
            let mut e = env(&pairs);
            host_trust(&home, &mut e);
            assert_eq!(e, env(&pairs), "{pairs:?}");
            assert!(!home.join(STORE_FILE).exists(), "{pairs:?}");
        }
        // The JVM splits the variable at each space: no store in such a home.
        let mut e = env(&[("SSL_CERT_FILE", p(&one))]);
        host_trust(&dir.join("a b"), &mut e);
        assert_eq!(e, env(&[("SSL_CERT_FILE", p(&one))]));
        assert!(!dir.join("a b").join(STORE_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
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
