//! `podrunner mint-serve` (GitHub #31): the remote mint on a machine that
//! is not a unit host. It holds the source and its credentials, and serves
//! the source wire to the pinned keys of the unit hosts over mutual TLS.

use std::collections::BTreeMap;
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use pod_core::clock::{Clock, SystemClock};
use pod_core::failure::Class;
use pod_tls::mutual::{Pin, Pins, server_config};
use pod_tls::server::{Served, certified};
use podrunner_remote::gate::Gate;
use podrunner_remote::ledger::Ledger;
use podrunner_remote::transport::{ALPN, HostKey, Server, provider};
use podrunner_slots::config::{SourceConfig, SourceKind};
use podrunner_tier::TierConfig;
use serde::Deserialize;

use crate::serve::{Failure, fail, private_file, source_of};

/// One unit host: the name in the ledger and the audit, the pin of its key,
/// and its slot count.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostConfig {
    pub name: String,
    pub pin: String,
    pub slots: u32,
    /// The end of the life of this key, in seconds of the Unix epoch. A
    /// rotation gives the host a second `[[host]]` with the new pin, then
    /// removes the old one.
    #[serde(default)]
    pub not_after_s: Option<u64>,
}

/// The file of `mint-serve`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MintConfig {
    /// The address to listen on (`0.0.0.0:8443`).
    pub listen: String,
    /// The certificate and the P-256 key (mode 0600) of the mint.
    pub cert_file: String,
    pub key_file: String,
    /// The directory of the ledger and the audit file.
    pub state: String,
    #[serde(default)]
    pub allow_fake: bool,
    pub source: SourceConfig,
    pub trust: TierConfig,
    #[serde(default)]
    pub host: Vec<HostConfig>,
    /// A credential with no start after `after_s` is an alert; at
    /// `revoke_after` alerts a host is revoked.
    #[serde(default = "after_s")]
    pub reconcile_after_s: u64,
    #[serde(default = "revoke_after")]
    pub revoke_after: usize,
}

fn after_s() -> u64 {
    podrunner_remote::gate::Reconcile::default().after_s
}

fn revoke_after() -> usize {
    podrunner_remote::gate::Reconcile::default().revoke_after
}

const ONE_TABLE: &str = "[source] trust: a remote mint takes one trust table, not one for each forge";

impl MintConfig {
    pub fn parse(text: &str) -> Result<MintConfig, String> {
        let c: MintConfig = toml::from_str(text).map_err(|e| e.to_string())?;
        if c.source.kind == SourceKind::Remote {
            return Err("[source]: the mint holds the source; a remote source is for a unit host".into());
        }
        if c.source.kind == SourceKind::Fake && !c.allow_fake {
            return Err("[source] is fake: set allow_fake = true".into());
        }
        if c.host.is_empty() {
            return Err("no [[host]]: no unit host can reach the mint".into());
        }
        podrunner_slots::check_personal(&c.source)?;
        if c.source.kind == SourceKind::GithubScaleset {
            podrunner_slots::check_scaleset(&c.source).map_err(|e| e.replacen("[[source]]", "[source]", 1))?;
        }
        // The mint computes the tier too: it refuses what serve refuses.
        c.trust.check()?;
        if let Some(t) = &c.source.trust {
            podrunner_slots::trust::table_check(t).map_err(|e| format!("[source] trust: {e}"))?;
            // A remote mint serves one source of one forge (T-494): `check`
            // refuses here what `run` cannot use.
            if matches!(t, podrunner_slots::trust::SourceTrust::ByForge(_)) {
                return Err(ONE_TABLE.into());
            }
        }
        let mut pins = std::collections::BTreeSet::new();
        for h in &c.host {
            let pin = Pin::parse(&h.pin).map_err(|e| format!("[[host]] {:?}: {e}", h.name))?;
            if !pins.insert(pin.to_string()) || h.slots == 0 {
                return Err(format!("[[host]] {:?}: a pin two times, or no slot", h.name));
            }
            // The keys of one host in a rotation: one slot count.
            if c.host.iter().any(|o| o.name == h.name && o.slots != h.slots) {
                return Err(format!("[[host]] {:?}: two slot counts", h.name));
            }
        }
        Ok(c)
    }
}

/// True for a file of `mint-serve`: it names the certificate of the mint.
/// `podrunner check` then checks it as a mint file (T-171).
pub fn is_mint_file(text: &str) -> bool {
    text.parse::<toml::Table>().is_ok_and(|t| t.contains_key("cert_file"))
}

/// `podrunner check`: a file of `mint-serve` as the mint reads it, else a
/// file of `serve` (T-171). The text of the answer.
pub fn check_any(path: &Path) -> Result<&'static str, Failure> {
    if is_mint_file(&std::fs::read_to_string(path).unwrap_or_default()) {
        check(path)?;
        return Ok("the mint configuration is usable (a file of mint-serve)\n");
    }
    let cfg = crate::serve::check(path)?;
    // The line of `serve` at its start for a kind that it drops (T-629).
    for (name, why) in crate::tiers::away(&cfg) {
        eprintln!("podrunner: [unit.{name}]: not here: {why}");
    }
    Ok("the configuration is usable (a file of serve)\n")
}

/// Check a mint file as `mint-serve` reads it, and start nothing: the keys,
/// the source (its key file and its program), and the certificate and the
/// key of the mint (T-171).
pub fn check(config: &Path) -> Result<MintConfig, Failure> {
    let text = std::fs::read_to_string(config)
        .map_err(|e| fail(Class::Config, format!("the configuration {}: {e}", config.display())))?;
    // A file of serve is not a file of the mint: say so, not a type error
    // at one of its keys (T-216).
    if text.parse::<toml::Table>().is_ok() && !is_mint_file(&text) {
        return Err(fail(
            Class::Config,
            format!(
                "{} is not a mint configuration: it has no cert_file (`podrunner check` reads a file of serve)",
                config.display()
            ),
        ));
    }
    let c = MintConfig::parse(&text).map_err(|e| fail(Class::Config, e))?;
    let mut notes = c.trust.notes().into_iter().map(|n| format!("[trust] {n}")).collect::<Vec<_>>();
    if let Some(t) = &c.source.trust {
        notes.extend(podrunner_slots::trust::table_notes("[source] trust", t));
    }
    notes.extend(podrunner_slots::personal_note(&c.source).map(|n| n.replacen("[[source]]", "[source]", 1)));
    for line in notes {
        eprintln!("podrunner: {line}");
    }
    source_of(&c.source, Arc::new(SystemClock), std::time::Instant::now(), None)?;
    private_file("the key file of the mint", &c.key_file)?;
    std::fs::read(&c.cert_file).map_err(|e| fail(Class::Config, format!("the certificate: {}", e)))?;
    Ok(c)
}

/// Run the mint until the process ends.
pub fn run(config: &Path) -> Result<(), Failure> {
    let c = check(config)?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    // A mint holds no remote source: no lookup.
    let source = source_of(&c.source, clock.clone(), clock.now(), None)?;
    let state = Path::new(&c.state);
    let io = |what: &str, e: std::io::Error| fail(Class::Unavailable, format!("{what}: {}", e));
    std::fs::create_dir_all(state).map_err(|e| io("the state directory", e))?;
    let ledger = Ledger::open(&state.join("ledger.jsonl")).map_err(|e| io("the ledger", e))?;
    let audit = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(state.join("audit.jsonl"))
    }
    .map_err(|e| io("the audit file", e))?;
    let hosts: BTreeMap<String, u32> = c.host.iter().map(|h| (h.name.clone(), h.slots)).collect();
    let reconcile = podrunner_remote::gate::Reconcile { after_s: c.reconcile_after_s, revoke_after: c.revoke_after };
    let now = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    // A remote mint serves one source of one forge: one table (T-494).
    let trust = match &c.source.trust {
        None => c.trust.clone(),
        Some(podrunner_slots::trust::SourceTrust::One(t)) => t.clone(),
        Some(podrunner_slots::trust::SourceTrust::ByForge(_)) => return Err(fail(Class::Config, ONE_TABLE)),
    };
    let gate = Gate::new(source, c.source.tier, trust, ledger, hosts, audit).with_reconcile(Box::new(now), reconcile);
    let keys: Vec<HostKey> = c
        .host
        .iter()
        .filter_map(|h| {
            Pin::parse(&h.pin).ok().map(|pin| HostKey { pin, host: h.name.clone(), not_after_s: h.not_after_s })
        })
        .collect();
    let key = private_file("the key file of the mint", &c.key_file)?;
    let cert = std::fs::read(&c.cert_file).map_err(|e| fail(Class::Config, format!("the certificate: {}", e)))?;
    let served = Served::new(Some(certified(&cert, &key).map_err(|e| fail(Class::Config, e.to_string()))?));
    let tls = server_config(provider(), served, Pins::new(keys.iter().map(|k| k.pin)), &[ALPN])
        .map_err(|e| fail(Class::Config, e.to_string()))?;
    let listener = TcpListener::bind(&c.listen).map_err(|e| io("the listen address", e))?;
    Server::new(Arc::new(Mutex::new(gate)), keys, tls).serve(listener, Arc::new(AtomicBool::new(false)));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIN: &str = "sha256/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

    fn file(extra: &str, hosts: &str) -> String {
        format!(
            "listen = \"127.0.0.1:0\"\ncert_file = \"c\"\nkey_file = \"k\"\nstate = \"s\"\n{extra}\n\
             [source]\nname = \"src\"\nkind = \"fake\"\ntier = \"trusted\"\narch = \"x86_64\"\n\
             [trust]\norganizations = [{{ login = \"example-org\" }}]\n{hosts}"
        )
    }

    #[test]
    fn the_file_of_the_mint_is_checked() {
        let host = format!("[[host]]\nname = \"h1\"\npin = \"{PIN}\"\nslots = 2\n");
        assert!(MintConfig::parse(&file("allow_fake = true", &host)).is_ok());
        assert!(MintConfig::parse(&file("", &host)).unwrap_err().contains("allow_fake"));
        assert!(MintConfig::parse(&file("allow_fake = true", "")).unwrap_err().contains("no [[host]]"));
        let bad = host.replace(PIN, "sha256/short");
        assert!(MintConfig::parse(&file("allow_fake = true", &bad)).is_err());
        let twice = format!("{host}{host}");
        assert!(MintConfig::parse(&file("allow_fake = true", &twice)).unwrap_err().contains("two times"));
        // A rotation: a second key of one host, with the same slots.
        let other = "sha256/AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        let rotation = format!("{host}not_after_s = 1800000000\n{}", host.replace(PIN, other));
        assert!(MintConfig::parse(&file("allow_fake = true", &rotation)).is_ok());
        let slots = format!("{host}{}", host.replace(PIN, other).replace("slots = 2", "slots = 3"));
        assert!(MintConfig::parse(&file("allow_fake = true", &slots)).unwrap_err().contains("two slot counts"));
        let remote = file("allow_fake = true", &host).replace("kind = \"fake\"", "kind = \"remote\"");
        assert!(MintConfig::parse(&remote).unwrap_err().contains("unit host"));
    }

    /// The T-504 review: the mint checks the `config_url` of its scale-set
    /// source as `serve` does.
    #[test]
    fn the_mint_checks_the_config_url_of_its_scale_set() {
        let host = format!("[[host]]\nname = \"h1\"\npin = \"{PIN}\"\nslots = 2\n");
        let scaleset = |keys: &str| {
            file("", &host).replace(
                "kind = \"fake\"",
                &format!(
                    "kind = \"github-scaleset\"\nsidecar = \"/bin/true\"\nscale_set = \"s\"\ntoken_file = \"t\"\n{keys}"
                ),
            )
        };
        assert!(MintConfig::parse(&scaleset("config_url = \"https://github.com/alice/app\"")).is_ok());
        let err =
            MintConfig::parse(&scaleset("config_url = \"https://github.com/alice\"\nallow_personal_account = true"))
                .unwrap_err();
        assert!(err.starts_with("[source] \"src\": allow_personal_account needs the URL of a repository"), "{err}");
        let err = MintConfig::parse(&scaleset("config_url = \"https://github.com/alice/app.git\"")).unwrap_err();
        assert!(err.contains(".git"), "{err}");
    }
}
