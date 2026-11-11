//! `podrunner serve`: build the supervisor from `podrunner.toml`, then
//! tick each second and answer the control socket between the ticks.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pod_core::failure::Class;
use podrunner_clock::{Clock, SystemClock};
use podrunner_fake::{FakeSource, FakeUnit, parse_script};
use podrunner_schema::{
    Boundary, Capability, ContainerRoute, CredentialStore, EgressHold, Fact, LimitMode, Principal, SecretDelivery,
    TreeKill,
};
use podrunner_slots::config::{Config, SourceKind};
use podrunner_slots::{Slots, Source, UnitKind};
use podrunner_unit::UnitDriver;
use serde_json::Value;

use crate::control::{self, Handler, Reply, Request};

/// A failure of `serve`, with its class of `pod-core`.
#[derive(Debug)]
pub struct Failure {
    pub class: Class,
    pub detail: String,
}

pub(crate) fn fail(class: Class, detail: impl Into<String>) -> Failure {
    Failure { class, detail: detail.into() }
}

/// The record of the `fake` kind: no boundary, a shared principal, no hold.
/// It meets only the floor of `local`: a fake unit never runs a forge job
/// of `trusted` or `untrusted`.
pub fn fake_capability() -> Capability {
    Capability {
        schema: podrunner_schema::SCHEMA,
        kind: "fake".into(),
        arch: std::env::consts::ARCH.into(),
        boundary: Boundary::None,
        boundary_proof: None,
        principal: Principal::Shared,
        container: ContainerRoute::None,
        net: "shared-host".into(),
        secret_delivery: vec![SecretDelivery::Files],
        credential_store: CredentialStore::PrivateDir,
        mount_modes: vec!["rw".into()],
        rootfs_fresh: Fact::Yes,
        tree_kill: TreeKill::Verified,
        limits: [("wall".to_string(), LimitMode::Enforced)].into(),
        egress_hold: EgressHold::None,
        kvm: Fact::Unknown,
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A file of a key: the group and the others cannot read it.
pub(crate) fn private_file(name: &str, path: &str) -> Result<Vec<u8>, Failure> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(|e| fail(Class::Config, format!("{name}: {}", e.kind())))?;
    if meta.permissions().mode() & 0o077 != 0 {
        return Err(fail(Class::Config, format!("{name} is open to the group or to others: set mode 0600")));
    }
    std::fs::read(path).map_err(|e| fail(Class::Config, format!("{name}: {}", e.kind())))
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
        .map_err(|e| fail(Class::Config, format!("{}: the certificate: {}", s.name, e.kind())))?;
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
) -> Result<podrunner_mint::SidecarSource, Failure> {
    use std::os::unix::fs::PermissionsExt;
    let get = |v: &Option<String>| v.clone().unwrap_or_default();
    let key_file = get(&s.key_file);
    let meta = std::fs::metadata(&key_file)
        .map_err(|e| fail(Class::Config, format!("{}: the key file: {}", s.name, e.kind())))?;
    if meta.permissions().mode() & 0o077 != 0 {
        return Err(fail(
            Class::Config,
            format!("{}: the key file is open to the group or to others: set mode 0400", s.name),
        ));
    }
    let pem = std::fs::read_to_string(&key_file)
        .map_err(|e| fail(Class::Config, format!("{}: the key file: {}", s.name, e.kind())))?;
    let key = podrunner_mint::sign::key_from_pem(&pem)
        .map_err(|_| fail(Class::Config, format!("{}: the key file holds no PKCS#1 key", s.name)))?;
    let signer = podrunner_mint::sign::Signer::new(key, &get(&s.app_id), clock, Box::new(now_unix));
    let mut args = vec![
        "-config-url".to_string(),
        get(&s.config_url),
        "-api-url".into(),
        s.api_url.clone().unwrap_or_else(|| "https://api.github.com".into()),
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
    if s.operator_controlled {
        args.push("-operator-controlled".into());
    }
    podrunner_slots::config::check_driver(Path::new(&get(&s.sidecar)))
        .map_err(|e| fail(Class::Config, format!("[[source]] {} sidecar: {e}", s.name)))?;
    let caller = format!("sidecar:{}", s.name);
    Ok(podrunner_mint::SidecarSource::new(&get(&s.sidecar), args, Arc::new(std::sync::Mutex::new(signer)), &caller))
}

/// The source of a `[[source]]` table, behind `mint` (GitHub #27): mint
/// stops a source for good after a revocation.
pub(crate) fn source_of(
    s: &podrunner_slots::config::SourceConfig,
    clock: Arc<dyn Clock>,
    lookup: Option<podrunner_remote::transport::Lookup>,
) -> Result<Box<dyn podrunner_source::JobSource>, Failure> {
    let source: Box<dyn podrunner_source::JobSource> = match s.kind {
        SourceKind::Fake => {
            let text = match &s.script {
                Some(p) => std::fs::read_to_string(p)
                    .map_err(|e| fail(Class::Config, format!("the script of {}: {}", s.name, e.kind())))?,
                None => String::new(),
            };
            let script =
                parse_script(&text).map_err(|e| fail(Class::Config, format!("the script of {}: {e}", s.name)))?;
            Box::new(FakeSource::new(&s.name, clock, script))
        }
        SourceKind::GithubScaleset => Box::new(scaleset(s, clock)?),
        SourceKind::Remote => Box::new(remote(s, lookup)?),
    };
    Ok(Box::new(podrunner_mint::Mint::new(&s.name, source)))
}

/// Check each part of `cfg` that has no side effect (GitHub #58): each
/// source is built and not called, and each unit kind has its driver
/// file. No state directory, no unit, no network. `serve` runs it before
/// `build`, so `check` and `serve` refuse a file with the same text.
pub fn plan(cfg: &Config) -> Result<(), Failure> {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    resolve(cfg)?;
    for s in &cfg.source {
        drop(source_of(s, clock.clone(), None)?);
    }
    for (name, u) in &cfg.unit {
        match name.as_str() {
            "fake" => {}
            "podvm" => {
                let binary = u.binary.clone().ok_or_else(|| fail(Class::Config, "[unit.podvm] needs binary"))?;
                podrunner_slots::config::check_driver(Path::new(&binary))
                    .map_err(|e| fail(Class::Config, format!("[unit.podvm] binary: {e}")))?;
                if !Path::new(&binary).is_file() {
                    return Err(fail(Class::Config, "[unit.podvm] binary: not a file"));
                }
                let images = u.images.clone().ok_or_else(|| fail(Class::Config, "[unit.podvm] needs images"))?;
                if !Path::new(&images).is_dir() {
                    return Err(fail(Class::Config, "[unit.podvm] images: not a directory"));
                }
            }
            other => {
                return Err(fail(Class::Unsupported, format!("[unit.{other}]: no driver of this kind in this build")));
            }
        }
    }
    Ok(())
}

/// Read and check the configuration at `path`: what `check` prints.
pub fn check(path: &Path) -> Result<Config, Failure> {
    let text =
        std::fs::read_to_string(path).map_err(|e| fail(Class::Config, format!("the configuration: {}", e.kind())))?;
    let cfg = Config::parse(&text).map_err(|e| fail(Class::Config, e))?;
    plan(&cfg)?;
    Ok(cfg)
}

/// The resolver of `[dns]` (T-076): `pod-dns`, with the pins of the file.
fn resolve(cfg: &Config) -> Result<Arc<dyn pod_core::Resolve>, Failure> {
    let pins =
        pod_dns::parse_pins(&cfg.dns.pins.join(",")).map_err(|e| fail(Class::Config, format!("[dns] pins: {e}")))?;
    Ok(podrunner_egress::dns::resolver(cfg.dns.doh, pins))
}

/// Build the supervisor of a configuration.
pub fn build(cfg: Config, state: &Path) -> Result<Slots, Failure> {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let resolve = resolve(&cfg)?;
    let lookup: podrunner_remote::transport::Lookup = Arc::new(podrunner_egress::dns::lookup(resolve.clone()));
    let mut sources = Vec::new();
    for s in &cfg.source {
        sources.push(Source {
            cfg: s.clone(),
            source: source_of(s, clock.clone(), Some(lookup.clone()))?,
            revoked: false,
        });
    }
    let mut kinds = Vec::new();
    let mut retests = Vec::new();
    let mut policies = std::collections::BTreeMap::new();
    for (name, u) in &cfg.unit {
        let (driver, capability): (Arc<dyn podrunner_unit::UnitDriver>, Capability) = match name.as_str() {
            "fake" => {
                let cap = fake_capability();
                let platform = if cap.arch == "aarch64" { "linux/arm64" } else { "linux/amd64" };
                (Arc::new(FakeUnit::new(&state.join("fake"), cap.clone(), platform)), cap)
            }
            "podvm" => {
                let need = |v: &Option<String>, key: &str| {
                    v.clone().ok_or_else(|| fail(Class::Config, format!("[unit.podvm] needs {key}")))
                };
                let binary = need(&u.binary, "binary")?;
                podrunner_slots::config::check_driver(Path::new(&binary))
                    .map_err(|e| fail(Class::Config, format!("[unit.podvm] binary: {e}")))?;
                let podvm = podrunner_podvm::Podvm { binary: binary.into() };
                let images = PathBuf::from(need(&u.images, "images")?);
                let mut unit = podrunner_podvm::PodvmUnit::new(podvm, &state.join("podvm"), &images)
                    .map_err(|e| fail(Class::Unsupported, format!("[unit.podvm]: {e}")))?
                    .with_runner_dir(u.runner_dir.as_deref().unwrap_or("/home/runner"));
                if let Some(podnet) = &u.podnet {
                    podrunner_slots::config::check_driver(Path::new(podnet))
                        .map_err(|e| fail(Class::Config, format!("[unit.podvm] podnet: {e}")))?;
                    unit = unit
                        .with_podnet(Path::new(podnet))
                        .map_err(|e| fail(Class::Unsupported, format!("[unit.podvm] podnet: {e}")))?;
                }
                // The tree kill is verified on this host, with the pinned
                // payload, before a wall limit counts as enforced.
                let arch = std::env::consts::ARCH;
                if let Some(pin) = cfg.pin.payload.iter().find(|p| p.arch == arch) {
                    let id = podrunner_schema::UnitId::parse("u-999999999").expect("a unit id");
                    match unit.verify_tree_kill(&pin.reference, id) {
                        Ok(true) => {}
                        Ok(false) => eprintln!(
                            "podrunner: [unit.podvm]: the tree kill is not proven: tree_kill stays unverified"
                        ),
                        Err(e) => eprintln!("podrunner: [unit.podvm]: the self-test of the tree kill: {e}"),
                    }
                }
                let cap = unit.probe().map_err(|e| fail(Class::Unsupported, e.to_string()))?;
                (Arc::new(unit), cap)
            }
            other => {
                return Err(fail(Class::Unsupported, format!("[unit.{other}]: no driver of this kind in this build")));
            }
        };
        // A claim of guest holds only on the allowlist and after the
        // self-test of GitHub #36; slots tests it again each day and on a
        // change of the driver file.
        let capability = if capability.boundary == podrunner_schema::Boundary::Guest {
            let allowed = cfg.guest_kinds.contains(name);
            let pin = cfg.pin.payload.iter().find(|p| p.arch == capability.arch);
            let platform = if capability.arch == "aarch64" { "linux/arm64" } else { "linux/amd64" };
            let verdict = match (allowed, pin) {
                (true, Some(p)) => podrunner_slots::guest::self_test(
                    driver.as_ref(),
                    &p.reference,
                    platform,
                    podrunner_slots::guest::test_ids(),
                    state,
                ),
                (true, None) => Err("no pinned payload for the self-test".into()),
                (false, _) => Err("not on guest_kinds".into()),
            };
            let claim = capability.clone();
            let (cap, note) = podrunner_slots::guest::settle(capability, allowed, verdict);
            if let Some(why) = note {
                eprintln!("podrunner: [unit.{name}]: the claim of guest drops to none: {why}");
            }
            if let Some(p) = pin {
                let driver = u.binary.as_ref().map(PathBuf::from);
                let stamp = driver.as_deref().and_then(podrunner_slots::guest::stamp);
                retests.push((
                    kinds.len(),
                    podrunner_slots::guest::Retest {
                        claim,
                        allowed,
                        payload: p.reference.clone(),
                        platform: platform.into(),
                        last_ms: clock.now_ms(),
                        driver,
                        stamp,
                    },
                ));
            }
            cap
        } else {
            capability
        };
        policies.insert(
            name.clone(),
            podrunner_hook::policy::Policy {
                container: capability.container,
                shared_host_net: capability.net == "shared-host",
                registries: u.registries.clone(),
                work_root: u.runner_dir.clone().unwrap_or_default(),
            },
        );
        kinds.push(UnitKind { name: name.clone(), cfg: u.clone(), driver, capability });
    }
    let ttl = Duration::from_secs(cfg.deadlines.running_s + cfg.deadlines.grace_s + cfg.deadlines.destroy_s);
    let proxy = match &cfg.upstream_proxy {
        Some(url) => {
            Some(pod_proxy::Proxy::parse(url).map_err(|e| fail(Class::Config, format!("upstream_proxy: {e}")))?)
        }
        None => None,
    };
    let egress = podrunner_egress::Egress::host(proxy, cfg.upstream_names_only, resolve)
        .map_err(|e| fail(Class::Unavailable, e.to_string()))?;
    // An upstream that takes names resolves them again: unless it refuses a
    // non-public name, no hold through it is enforced (podcrates call C51).
    if cfg.upstream_names_only {
        let check = egress.check_upstream();
        if check != "refuses-non-public" {
            eprintln!("podrunner: the upstream proxy: {check}: each hold is advisory at most, no untrusted job");
            for k in &mut kinds {
                k.capability.egress_hold = cap_hold(k.capability.egress_hold);
            }
        }
    }
    let mut slots = Slots::start(cfg, state, clock, Box::new(now_unix), sources, kinds)
        .map_err(|e| fail(Class::Unavailable, format!("the state directory: {}", e.kind())))?;
    for (i, r) in retests {
        slots.retest_guest(i, r);
    }
    let hooks = crate::hooks::HooksOf::new(policies);
    slots.set_hooks(Box::new(hooks));
    Ok(slots.with_egress(Box::new(EgressOf { egress, ttl })))
}

/// The hold of a kind behind an upstream that does not refuse a
/// non-public name: `advisory` at most.
pub(crate) fn cap_hold(h: podrunner_schema::EgressHold) -> podrunner_schema::EgressHold {
    match h {
        podrunner_schema::EgressHold::Enforced => podrunner_schema::EgressHold::Advisory,
        other => other,
    }
}

/// The egress module behind slots.
struct EgressOf {
    egress: podrunner_egress::Egress,
    ttl: Duration,
}

impl podrunner_slots::EgressPort for EgressOf {
    fn open(&self, unit: &str, policy: &str, socket: &Path) -> Result<usize, String> {
        self.egress.open(unit, policy, socket, self.ttl).map_err(|e| e.to_string())
    }
    fn close(&self, unit: &str) {
        self.egress.close(unit);
    }
    fn alive(&self) -> bool {
        self.egress.alive()
    }
}

struct Supervisor {
    slots: Slots,
    stop: bool,
}

impl Handler for Supervisor {
    fn handle(&mut self, req: &Request) -> Reply {
        match req {
            Request::Version => Reply::Ok { value: Value::from(crate::cli::version().trim_end()) },
            Request::Status => match serde_json::to_value(self.slots.status()) {
                Ok(v) => Reply::Ok { value: v },
                Err(_) => Reply::error("internal", "the status is not JSON"),
            },
            Request::Drain => {
                self.slots.drain();
                Reply::Ok { value: Value::from("draining") }
            }
            Request::Stop => {
                self.slots.drain();
                self.stop = true;
                Reply::Ok { value: Value::from("stopping") }
            }
            Request::Capacity => Reply::Ok {
                value: Value::from(self.slots.status().kinds.iter().map(|k| k.slots - k.busy).sum::<u32>()),
            },
            Request::ClearDegraded => {
                self.slots.clear_degraded();
                Reply::Ok { value: Value::from("cleared") }
            }
            Request::Withdraw { .. } => {
                Reply::error("unsupported", "withdraw of one job comes with the scale-set source (GitHub #29)")
            }
        }
    }
}

/// The default path of the control socket.
pub fn socket_of(state: &Path) -> PathBuf {
    state.join("ctl.sock")
}

/// Run until `stop` or a stop signal, and until each job is final after
/// it. The lock of the state directory comes first (GitHub #54): the
/// replay of the journal destroys the open units, so it must never run
/// beside a live `serve`.
pub fn run(config: &Path, state: &Path, socket: &Path) -> Result<(), Failure> {
    let _lock = crate::lock::StateLock::take(state)?;
    let cfg = check(config)?;
    let slots = build(cfg, state)?;
    if slots.no_tier_served() {
        eprintln!("podrunner: no-tier-served: no unit kind meets the floor of a source");
    }
    let listener = control::bind(socket).map_err(|e| {
        let class = if e.kind() == std::io::ErrorKind::AlreadyExists { Class::Usage } else { Class::Unavailable };
        fail(class, format!("the control socket: {e}"))
    })?;
    let calls = control::spawn(listener);
    crate::notify::install();
    crate::notify::send("READY=1");
    let watchdog = crate::notify::watchdog();
    let mut sup = Supervisor { slots, stop: false };
    let (mut next, mut ping, mut stopping) = (Instant::now(), Instant::now(), false);
    loop {
        if crate::notify::take_signal() && !sup.stop {
            sup.handle(&Request::Stop);
        }
        if sup.stop && !stopping {
            crate::notify::send("STOPPING=1");
            stopping = true;
        }
        if Instant::now() >= next {
            sup.slots.tick();
            next += Duration::from_secs(1);
        }
        if let Some(w) = watchdog
            && ping.elapsed() >= w
        {
            crate::notify::send("WATCHDOG=1");
            ping = Instant::now();
        }
        control::answer_waiting(&calls, &mut sup, Duration::from_millis(50));
        if sup.stop && sup.slots.jobs().all(|j| j.state.is_final()) {
            let _ = std::fs::remove_file(socket);
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use podrunner_schema::EgressHold;

    #[test]
    fn an_upstream_of_names_caps_the_hold_at_advisory() {
        assert_eq!(super::cap_hold(EgressHold::Enforced), EgressHold::Advisory);
        assert_eq!(super::cap_hold(EgressHold::Advisory), EgressHold::Advisory);
        assert_eq!(super::cap_hold(EgressHold::None), EgressHold::None);
        let e =
            podrunner_slots::config::Config::parse("host = \"h\"\nupstream_names_only = true\n[pin]\n").unwrap_err();
        assert!(e.contains("needs upstream_proxy"), "{e}");
        assert!(
            podrunner_slots::config::Config::parse(
                "host = \"h\"\nupstream_proxy = \"http://127.0.0.1:3128\"\nupstream_names_only = true\n[pin]\n"
            )
            .is_ok()
        );
    }
}
