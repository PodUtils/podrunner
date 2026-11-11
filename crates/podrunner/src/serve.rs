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
            SourceKind::GithubScaleset => {
                return Err(fail(
                    Class::Unsupported,
                    format!("{}: the github-scaleset source comes with GitHub #29", s.name),
                ));
            }
        };
        // mint stands between slots and each source (GitHub #27).
        let source: Box<dyn podrunner_source::JobSource> = Box::new(podrunner_mint::Mint::new(&s.name, source));
        sources.push(Source { cfg: s.clone(), source, revoked: false });
    }
    let mut kinds = Vec::new();
    for (name, u) in &cfg.unit {
        let (driver, capability): (Box<dyn podrunner_unit::UnitDriver>, Capability) = match name.as_str() {
            "fake" => {
                let cap = fake_capability();
                let platform = if cap.arch == "aarch64" { "linux/arm64" } else { "linux/amd64" };
                (Box::new(FakeUnit::new(&state.join("fake"), cap.clone(), platform)), cap)
            }
            "podvm" => {
                let need = |v: &Option<String>, key: &str| {
                    v.clone().ok_or_else(|| fail(Class::Config, format!("[unit.podvm] needs {key}")))
                };
                let podvm = podrunner_podvm::Podvm { binary: need(&u.binary, "binary")?.into() };
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
                (Box::new(unit), cap)
            }
            other => {
                return Err(fail(Class::Unsupported, format!("[unit.{other}]: no driver of this kind in this build")));
            }
        };
        kinds.push(UnitKind { name: name.clone(), cfg: u.clone(), driver, capability });
    }
    Slots::start(cfg, state, clock, Box::new(now_unix), sources, kinds)
        .map_err(|e| fail(Class::Unavailable, format!("the state directory: {}", e.kind())))
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
