//! The sources of `serve`: the job source of each `[[source]]` table.

use std::path::Path;
use std::sync::Arc;

use pod_core::clock::Clock;
use pod_core::failure::Class;
use podrunner_fake::{FakeSource, parse_script};
use podrunner_slots::config::SourceKind;

use super::{Failure, fail, now_unix};

/// A file of a key: the group and the others cannot read it.
pub(crate) fn private_file(name: &str, path: &str) -> Result<Vec<u8>, Failure> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(|e| fail(Class::Config, format!("{name}: {}", e)))?;
    if meta.permissions().mode() & 0o077 != 0 {
        return Err(fail(Class::Config, format!("{name} is open to the group or to others: set mode 0600")));
    }
    std::fs::read(path).map_err(|e| fail(Class::Config, format!("{name}: {}", e)))
}

/// The wire to a remote mint (GitHub #31): this host presents its key, and
/// trusts only the pinned key of the mint.
fn remote(
    s: &podrunner_slots::config::SourceConfig,
    lookup: Option<podrunner_remote::transport::Lookup>,
) -> Result<podrunner_source::Wire<podrunner_remote::transport::Https>, Failure> {
    use podrunner_remote::transport::{ALPN, Https, provider};
    let get = |v: &Option<String>| v.clone().unwrap_or_default();
    let key = private_file(&format!("{}: the key file", s.name), &get(&s.key_file))?;
    let cert = std::fs::read(get(&s.cert_file))
        .map_err(|e| fail(Class::Config, format!("{}: the certificate: {}", s.name, e)))?;
    let certified =
        pod_tls::server::certified(&cert, &key).map_err(|e| fail(Class::Config, format!("{}: {e}", s.name)))?;
    let pin =
        pod_tls::mutual::Pin::parse(&get(&s.mint_pin)).map_err(|e| fail(Class::Config, format!("{}: {e}", s.name)))?;
    let config = pod_tls::mutual::client_config(provider(), certified, pod_tls::mutual::Pins::new([pin]), &[ALPN])
        .map_err(|e| fail(Class::Config, format!("{}: {e}", s.name)))?;
    Ok(podrunner_source::Wire(Https { address: get(&s.mint), config, lookup }))
}

/// The scale-set sidecar of a source, with `sign` over the key of the App.
pub(crate) fn scaleset(
    s: &podrunner_slots::config::SourceConfig,
    clock: Arc<dyn Clock>,
    origin: std::time::Instant,
) -> Result<podrunner_mint::SidecarSource, Failure> {
    use std::os::unix::fs::PermissionsExt;
    if s.token_file.is_some() {
        return crate::pat::scaleset(s);
    }
    let get = |v: &Option<String>| v.clone().unwrap_or_default();
    let key_file = get(&s.key_file);
    let meta =
        std::fs::metadata(&key_file).map_err(|e| fail(Class::Config, format!("{}: the key file: {}", s.name, e)))?;
    if meta.permissions().mode() & 0o077 != 0 {
        return Err(fail(
            Class::Config,
            format!("{}: the key file is open to the group or to others: set mode 0400", s.name),
        ));
    }
    let pem = std::fs::read_to_string(&key_file)
        .map_err(|e| fail(Class::Config, format!("{}: the key file: {}", s.name, e)))?;
    let key = podrunner_mint::sign::key_from_pem(&pem).map_err(|e| {
        let why = match e {
            podrunner_mint::sign::Refusal::Key(why) => why,
            other => format!("{other:?}"),
        };
        fail(Class::Config, format!("{}: the key file: {why}", s.name))
    })?;
    // Each request of `sign` is a line of the log of the process (T-172).
    let signer = podrunner_mint::sign::Signer::new(key, &get(&s.app_id), clock, Box::new(now_unix))
        .since(origin)
        .with_sink(Box::new(std::io::stderr()));
    let mut args = vec![
        "-config-url".to_string(),
        get(&s.config_url),
        "-api-url".into(),
        podrunner_slots::config_url::api_url(s),
        "-installation-id".into(),
        s.installation_id.unwrap_or(0).to_string(),
        "-scale-set".into(),
        get(&s.scale_set),
        "-source".into(),
        s.name.clone(),
        "-arch".into(),
        s.arch.clone(),
        "-app-id".into(),
        get(&s.app_id),
    ];
    args.extend(flags(s));
    podrunner_slots::config::check_driver(Path::new(&get(&s.sidecar)))
        .map_err(|e| fail(Class::Config, format!("[[source]] {} sidecar: {e}", s.name)))?;
    let caller = format!("sidecar:{}", s.name);
    Ok(podrunner_mint::SidecarSource::new(&get(&s.sidecar), args, Arc::new(std::sync::Mutex::new(signer)), &caller))
}

/// The flags of the opt-ins of a scale-set source: the full report, and a
/// repository of a personal account (T-504).
pub(crate) fn flags(s: &podrunner_slots::config::SourceConfig) -> Vec<String> {
    let on = [(s.operator_controlled, "-operator-controlled"), (s.allow_personal_account, "-personal-account")];
    on.into_iter().filter(|(set, _)| *set).map(|(_, flag)| flag.to_string()).collect()
}

/// The source of a `[[source]]` table, behind `mint` (GitHub #27): mint
/// stops a source for good after a revocation. Each time in ms of the
/// source counts from `origin`, as each time of the slots does.
pub(crate) fn source_of(
    s: &podrunner_slots::config::SourceConfig,
    clock: Arc<dyn Clock>,
    origin: std::time::Instant,
    lookup: Option<podrunner_remote::transport::Lookup>,
) -> Result<Box<dyn podrunner_source::JobSource>, Failure> {
    let source: Box<dyn podrunner_source::JobSource> = match s.kind {
        SourceKind::Fake => {
            let text = match &s.script {
                Some(p) => std::fs::read_to_string(p)
                    .map_err(|e| fail(Class::Config, format!("the script of {}: {}", s.name, e)))?,
                None => String::new(),
            };
            let script =
                parse_script(&text).map_err(|e| fail(Class::Config, format!("the script of {}: {e}", s.name)))?;
            Box::new(FakeSource::new(&s.name, clock, script))
        }
        SourceKind::GithubScaleset => Box::new(scaleset(s, clock, origin)?),
        SourceKind::Remote => Box::new(remote(s, lookup)?),
        SourceKind::Program => Box::new(crate::program_source::program(s)?),
    };
    Ok(Box::new(podrunner_mint::Mint::new(&s.name, source)))
}

#[cfg(test)]
mod tests {
    /// T-504: the sidecar serves a personal account only with the key on.
    #[test]
    fn the_sidecar_gets_the_opt_ins_of_its_source() {
        let source = |keys: &str| -> podrunner_slots::config::SourceConfig {
            toml::from_str(&format!(
                "name = \"s\"\nkind = \"github-scaleset\"\ntier = \"trusted\"\narch = \"x86_64\"\n{keys}"
            ))
            .unwrap()
        };
        assert!(super::flags(&source("")).is_empty());
        assert!(super::flags(&source("allow_personal_account = false\n")).is_empty());
        assert_eq!(super::flags(&source("allow_personal_account = true\n")), ["-personal-account"]);
        assert_eq!(
            super::flags(&source("operator_controlled = true\nallow_personal_account = true\n")),
            ["-operator-controlled", "-personal-account"]
        );
    }
}
