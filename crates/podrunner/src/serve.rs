//! `podrunner serve`: build the supervisor from `podrunner.toml`, then
//! tick each second and answer the control socket between the ticks.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pod_core::clock::{Clock, SystemClock};
use pod_core::failure::Class;
use podrunner_fake::FakeUnit;
use podrunner_schema::Capability;
use podrunner_slots::config::Config;
use podrunner_slots::{Slots, Source, UnitKind};
use serde_json::Value;

use crate::control::{self, Handler, Reply, Request};

/// A failure of `serve`, with its class of `pod-core`.
#[derive(Debug)]
pub struct Failure {
    pub class: Class,
    pub detail: String,
}

pub use fake::{fake_capability, fake_capability_of};

pub(crate) fn fail(class: Class, detail: impl Into<String>) -> Failure {
    Failure { class, detail: detail.into() }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

mod fake;
#[path = "serve/front.rs"]
pub(crate) mod front;
#[path = "serve/host.rs"]
mod host;
mod restart;
#[path = "serve/sources.rs"]
mod sources;
pub(crate) use sources::{private_file, source_of};

/// Check each part of `cfg` that has no side effect (GitHub #58): each
/// source is built and not called, and each unit kind has its driver
/// file. No state directory, no unit, no network. `serve` runs it before
/// `build`, so `check` and `serve` refuse a file with the same text.
pub fn plan(cfg: &Config) -> Result<(), Failure> {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    resolve(cfg)?;
    if let Some(url) = &cfg.upstream_proxy {
        pod_proxy::Proxy::parse(url).map_err(|e| fail(Class::Config, format!("upstream_proxy: {e}")))?;
    }
    for s in &cfg.source {
        drop(source_of(s, clock.clone(), clock.now(), None)?);
    }
    for (name, u) in &cfg.unit {
        match name.as_str() {
            "fake" => {}
            "podvm" => crate::podvm_kind::plan(u)?,
            "microvm" => crate::microvm_kind::plan(u)?,
            "incus" => crate::incus_kind::plan(u)?,
            other => {
                return Err(fail(Class::Unsupported, format!("[unit.{other}]: no driver of this kind in this build")));
            }
        }
    }
    // A pinned image larger than the root disk of the incus kind (T-551).
    if let Some(u) = cfg.unit.get("incus") {
        crate::incus_kind::pins_fit(u, &cfg.pin)?;
    }
    crate::tiers::served(cfg)?;
    Ok(())
}

/// Read and check the configuration at `path`: what `check` prints.
pub fn check(path: &Path) -> Result<Config, Failure> {
    crate::tiers::file_closed(path)?;
    let text = std::fs::read_to_string(path)
        .map_err(|e| fail(Class::Config, format!("the configuration {}: {e}", path.display())))?;
    let cfg = Config::parse(&text).map_err(|e| fail(Class::Config, e))?;
    plan(&cfg)?;
    for line in crate::tiers::future_pins(&cfg, now_unix()) {
        eprintln!("podrunner: {line}");
    }
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
    build_with(cfg, state).map(|(slots, _)| slots)
}

/// Build the supervisor, and what builds one of its sources again.
fn build_with(cfg: Config, state: &Path) -> Result<(Slots, restart::Rebuild), Failure> {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    // One origin for the slots and the signer: their lines of `t_ms` share
    // one time line.
    let origin = clock.now();
    let resolve = resolve(&cfg)?;
    let lookup: podrunner_remote::transport::Lookup = Arc::new(podrunner_egress::dns::lookup(resolve.clone()));
    let rebuild = restart::Rebuild::new(&cfg, clock.clone(), origin, Some(lookup.clone()));
    let mut sources = Vec::new();
    for s in &cfg.source {
        sources.push(Source {
            cfg: s.clone(),
            source: source_of(s, clock.clone(), origin, Some(lookup.clone()))?,
            revoked: false,
        });
    }
    let mut kinds = Vec::new();
    let mut retests = Vec::new();
    let mut policies = std::collections::BTreeMap::new();
    for (name, u) in &cfg.unit {
        let (driver, capability): (Arc<dyn podrunner_unit::UnitDriver>, Capability) = match name.as_str() {
            "fake" => {
                let cap = fake_capability_of(u);
                if let Some(c) = u.claim {
                    eprintln!(
                        "podrunner: [unit.fake] claims {c:?} and an enforced hold: a test only, it holds nothing"
                    );
                }
                let platform = if cap.arch == "aarch64" { "linux/arm64" } else { "linux/amd64" };
                (Arc::new(FakeUnit::new(&state.join("fake"), cap.clone(), platform)), cap)
            }
            "podvm" => crate::podvm_kind::build(u, &cfg, state)?,
            "microvm" => crate::microvm_kind::build(u, state)?,
            "incus" => match crate::doctor::incusd() {
                Ok(()) => crate::incus_kind::build(u, state)?,
                // No incusd: no incus kind, and each other kind runs (T-534).
                Err(why) => {
                    eprintln!("podrunner: [unit.incus]: not here: {why}");
                    continue;
                }
            },
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
                        // The time of the slots starts after this test.
                        last_ms: 0,
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
                work_root: crate::podvm_kind::work_root(name, u),
            },
        );
        // slots measures the disk of a kind with a bound (T-074).
        let mut capability = capability;
        if u.disk_max_bytes.is_some() {
            capability.limits.entry("disk".into()).or_insert(podrunner_schema::LimitMode::Reported);
        }
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
        .map_err(|e| fail(Class::Unavailable, format!("the state directory: {}", e)))?
        .since(origin);
    for (i, r) in retests {
        slots.retest_guest(i, r);
    }
    let hooks = crate::hooks::HooksOf::new(policies);
    slots.set_hooks(Box::new(hooks));
    Ok((slots.with_egress(Box::new(EgressOf::new(egress, ttl))), rebuild))
}

/// The hold of a kind behind an upstream that does not refuse a
/// non-public name: `advisory` at most.
pub(crate) fn cap_hold(h: podrunner_schema::EgressHold) -> podrunner_schema::EgressHold {
    match h {
        podrunner_schema::EgressHold::Enforced => podrunner_schema::EgressHold::Advisory,
        other => other,
    }
}

/// The egress module behind slots. The module of a unit listens on an
/// inner socket; the socket that the unit sees is a front that also
/// carries the questions of its job-started hook (T-493).
struct EgressOf {
    egress: podrunner_egress::Egress,
    ttl: Duration,
    asks: std::sync::mpsc::SyncSender<podrunner_slots::assigned::Question>,
    asked: std::sync::Mutex<std::sync::mpsc::Receiver<podrunner_slots::assigned::Question>>,
    fronts: std::sync::Mutex<std::collections::HashMap<String, front::Front>>,
}

impl EgressOf {
    fn new(egress: podrunner_egress::Egress, ttl: Duration) -> EgressOf {
        // A bounded queue: a unit that asks too much gets `end`.
        let (asks, asked) = std::sync::mpsc::sync_channel(64);
        EgressOf { egress, ttl, asks, asked: std::sync::Mutex::new(asked), fronts: Default::default() }
    }
}

/// The time that a question waits for the slots; then the job ends (fail
/// closed).
const ASK_WAIT: Duration = Duration::from_secs(10);

impl podrunner_slots::EgressPort for EgressOf {
    fn open(&self, unit: &str, policy: &str, socket: &Path) -> Result<usize, String> {
        let id = podrunner_schema::UnitId::parse(unit).map_err(|e| e.to_string())?;
        let inner = socket.with_file_name("egress-inner.sock");
        let flagged = self.egress.open(unit, policy, &inner, self.ttl).map_err(|e| e.to_string())?;
        let asks = std::sync::Mutex::new(self.asks.clone());
        let ask: front::Ask = std::sync::Arc::new(move |body: &[u8]| {
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let sent = asks.lock().map(|a| a.try_send((id.clone(), body.to_vec(), tx)).is_ok()).unwrap_or(false);
            match sent.then(|| rx.recv_timeout(ASK_WAIT).ok()).flatten() {
                Some(answer) => answer,
                None => r#"{"verdict":"end","code":"source-unavailable"}"#.to_string(),
            }
        });
        match front::serve(socket, inner, ask, front::Bounds::default()) {
            Ok(f) => {
                if let Ok(mut m) = self.fronts.lock() {
                    m.insert(unit.to_string(), f);
                }
                Ok(flagged)
            }
            Err(e) => {
                self.egress.close(unit);
                Err(format!("the front of the egress socket: {}", e.kind()))
            }
        }
    }
    fn close(&self, unit: &str) {
        if let Ok(mut m) = self.fronts.lock() {
            m.remove(unit);
        }
        self.egress.close(unit);
    }
    fn alive(&self) -> bool {
        self.egress.alive()
    }
    fn lines(&self) -> Vec<(String, serde_json::Map<String, serde_json::Value>)> {
        self.egress.take_lines()
    }
    fn questions(&self) -> Vec<podrunner_slots::assigned::Question> {
        self.asked.lock().map(|r| r.try_iter().collect()).unwrap_or_default()
    }
}

struct Supervisor {
    slots: Slots,
    rebuild: restart::Rebuild,
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
            Request::RestartSource { source } => match self.rebuild.restart(&mut self.slots, source) {
                Ok(()) => Reply::Ok { value: Value::from("restarted") },
                Err(e) => Reply::error("refused", e),
            },
            Request::StopSource { source } => match self.slots.stop_source(source) {
                Ok(()) => Reply::Ok { value: Value::from("stopping") },
                Err(e) => Reply::error("refused", e),
            },
            Request::DrainSource { source } | Request::ResumeSource { source } => {
                let on = matches!(req, Request::DrainSource { .. });
                // A stopped source has no program: build it again first.
                if !on
                    && self.slots.source_ended(source)
                    && let Err(e) = self.rebuild.restart(&mut self.slots, source)
                {
                    return Reply::error("refused", e);
                }
                match self.slots.drain_source(source, on) {
                    Ok(()) => Reply::Ok { value: Value::from(if on { "draining" } else { "resumed" }) },
                    Err(e) => Reply::error("refused", e),
                }
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
    // A configuration that cannot be used fails before the state directory
    // is made (T-339 item 19).
    let cfg = check(config)?;
    crate::tiers::state_path(&cfg, state)?;
    let _lock = crate::lock::StateLock::take(state)?;
    let tools = host::tools(&cfg);
    let (mut slots, mut rebuild) = build_with(cfg, state)?;
    if slots.no_tier_served() {
        eprintln!("podrunner: no-tier-served: no unit kind meets the floor of a source");
    }
    // Each source learns what this host can run (T-495).
    let facts = host::facts(&slots, tools);
    for why in slots.hello_sources("podrunner", env!("CARGO_PKG_VERSION"), &facts) {
        eprintln!("podrunner: hello: {why}");
    }
    rebuild.facts = Some(facts);
    let listener = control::bind(socket).map_err(|e| {
        let class = if e.kind() == std::io::ErrorKind::AlreadyExists { Class::Usage } else { Class::Unavailable };
        fail(class, format!("the control socket: {e}"))
    })?;
    let snapshot = control::Snapshot::default();
    let calls = control::spawn_with(listener, snapshot.clone());
    crate::notify::install();
    crate::notify::send("READY=1");
    let watchdog = crate::notify::watchdog();
    let mut sup = Supervisor { slots, rebuild, stop: false };
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
            for line in sup.rebuild.watch(&mut sup.slots) {
                eprintln!("podrunner: source {line}");
            }
            for name in sup.slots.end_stopped() {
                eprintln!(
                    "podrunner: source {name}: stopped (its last job ended); resume --source {name} builds it again"
                );
            }
            next = next_tick(next, Instant::now());
            // The `status` for a client while a later tick waits on a
            // source (T-184).
            let status = control::Handler::handle(&mut sup, &Request::Status);
            if let Ok(mut s) = snapshot.lock() {
                *s = Some(status);
            }
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

/// The time of the tick after the one due at `due`. A late tick (a slow
/// call of a source) does not make the missed ticks run back to back: each
/// one polls the sources, and a burst of polls hits the mint (T-164).
fn next_tick(due: Instant, now: Instant) -> Instant {
    let next = due + Duration::from_secs(1);
    if next <= now { now + Duration::from_secs(1) } else { next }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use podrunner_schema::EgressHold;

    #[test]
    fn a_late_tick_does_not_run_the_missed_ticks() {
        let t0 = Instant::now();
        // On time: one second after the tick that was due.
        assert_eq!(super::next_tick(t0, t0 + Duration::from_millis(10)), t0 + Duration::from_secs(1));
        // Fifteen seconds late: the next tick is one second from now, not at
        // once.
        let now = t0 + Duration::from_secs(15);
        assert_eq!(super::next_tick(t0, now), now + Duration::from_secs(1));
    }

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
