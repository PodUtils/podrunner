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

fn fail(class: Class, detail: impl Into<String>) -> Failure {
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
        net: "host".into(),
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

/// The scale-set sidecar of a source, with `sign` over the key of the App.
fn scaleset(
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

/// Build the supervisor of a configuration.
pub fn build(cfg: Config, state: &Path) -> Result<Slots, Failure> {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut sources = Vec::new();
    for s in &cfg.source {
        let source: Box<dyn podrunner_source::JobSource> = match s.kind {
            SourceKind::Fake => {
                let text = match &s.script {
                    Some(p) => std::fs::read_to_string(p)
                        .map_err(|e| fail(Class::Config, format!("the script of {}: {}", s.name, e.kind())))?,
                    None => String::new(),
                };
                let script =
                    parse_script(&text).map_err(|e| fail(Class::Config, format!("the script of {}: {e}", s.name)))?;
                Box::new(FakeSource::new(&s.name, clock.clone(), script))
            }
            SourceKind::GithubScaleset => Box::new(scaleset(s, clock.clone())?),
        };
        // mint stands between slots and each source (GitHub #27).
        let source: Box<dyn podrunner_source::JobSource> = Box::new(podrunner_mint::Mint::new(&s.name, source));
        sources.push(Source { cfg: s.clone(), source, revoked: false });
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
                    .map_err(|e| fail(Class::Unsupported, format!("[unit.podvm]: {e}")))?;
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
    let egress = podrunner_egress::Egress::host(None).map_err(|e| fail(Class::Unavailable, e.to_string()))?;
    let mut slots = Slots::start(cfg, state, clock, Box::new(now_unix), sources, kinds)
        .map_err(|e| fail(Class::Unavailable, format!("the state directory: {}", e.kind())))?;
    for (i, r) in retests {
        slots.retest_guest(i, r);
    }
    let hooks = crate::hooks::HooksOf::new(policies);
    slots.set_hooks(Box::new(hooks));
    Ok(slots.with_egress(Box::new(EgressOf { egress, ttl })))
}

/// The egress module behind slots.
struct EgressOf {
    egress: podrunner_egress::Egress,
    ttl: Duration,
}

impl podrunner_slots::EgressPort for EgressOf {
    fn open(&self, unit: &str, policy: &str, socket: &Path) -> Result<(), String> {
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

/// Run until `stop`, and until each job is final after it.
pub fn run(config: &Path, state: &Path, socket: &Path) -> Result<(), Failure> {
    let text =
        std::fs::read_to_string(config).map_err(|e| fail(Class::Config, format!("the configuration: {}", e.kind())))?;
    let cfg = Config::parse(&text).map_err(|e| fail(Class::Config, e))?;
    let slots = build(cfg, state)?;
    if slots.no_tier_served() {
        eprintln!("podrunner: no-tier-served: no unit kind meets the floor of a source");
    }
    let listener =
        control::bind(socket).map_err(|e| fail(Class::Unavailable, format!("the control socket: {}", e.kind())))?;
    let mut sup = Supervisor { slots, stop: false };
    let mut next = Instant::now();
    loop {
        if Instant::now() >= next {
            sup.slots.tick();
            next += Duration::from_secs(1);
        }
        control::serve_waiting(&listener, &mut sup);
        if sup.stop && sup.slots.jobs().all(|j| j.state.is_final()) {
            let _ = std::fs::remove_file(socket);
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
