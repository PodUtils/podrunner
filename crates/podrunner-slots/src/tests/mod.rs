//! Level 1: the slots on the fake source and the fake unit, on a manual
//! clock. The fake unit runs real processes; each tick moves the clock by
//! one second and waits a few milliseconds of real time for them.

mod egress;
mod guards;
mod level1;
mod status;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use podrunner_clock::ManualClock;
use podrunner_fake::{FakeSource, FakeUnit, parse_script};
use podrunner_schema::{
    Boundary, Capability, ContainerRoute, CredentialStore, EgressHold, Fact, LimitMode, Principal, SecretDelivery,
    TreeKill,
};
use serde_json::{Value, json};

use crate::config::Config;
use crate::state::State;
use crate::{Slots, Source, UnitKind};

pub(crate) const PIN: &str = "runner@sha256:1111111111111111111111111111111111111111111111111111111111111111";
pub(crate) const NOW_UNIX: u64 = 1_800_000_000;

pub(crate) struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new() -> TempDir {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "podrunner-slots-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A capability of the fake unit.
pub(crate) fn capability(boundary: Boundary, principal: Principal, hold: EgressHold) -> Capability {
    Capability {
        schema: 1,
        kind: "fake".into(),
        arch: "x86_64".into(),
        boundary,
        boundary_proof: None,
        principal,
        container: ContainerRoute::None,
        net: "broker".into(),
        secret_delivery: vec![SecretDelivery::Files],
        credential_store: CredentialStore::PrivateDir,
        mount_modes: vec!["rw".into()],
        rootfs_fresh: Fact::Yes,
        tree_kill: TreeKill::Verified,
        limits: BTreeMap::from([("wall".into(), LimitMode::Enforced)]),
        egress_hold: hold,
        kvm: Fact::No,
    }
}

/// A full offer of `job-offer.v1` as a step of a fake script, with the
/// fields of `with` put over it.
pub(crate) fn offer(at_s: u64, job: &str, with: Value) -> String {
    let mut o = json!({
        "schema": 1, "job_id": job, "source": "src", "labels": ["podrunner"], "arch": "x86_64", "wait_deadline_s": 600,
        "trust_inputs": {
            "event": "push", "head_is_fork": "no", "actor_class": "member", "repo_visibility": "private",
            "owner": "example-org", "repo": "app", "run_id": 1, "ref": "refs/heads/main", "workflow_ref": "w"
        }
    });
    for (k, v) in with.as_object().unwrap() {
        if k == "trust_inputs" {
            for (tk, tv) in v.as_object().unwrap() {
                o["trust_inputs"][tk] = tv.clone();
            }
        } else {
            o[k] = v.clone();
        }
    }
    json!({ "at_s": at_s, "offer": o }).to_string()
}

/// The configuration of the tests: the floors of the tiers come from the
/// file (`trusted` at `confined`).
pub(crate) fn config(extra: &str) -> Config {
    Config::parse(&format!(
        r#"
host = "host-a"
allow_fake = true
{extra}
[trust]
organizations = [{{ login = "example-org" }}]
[pin]
[[pin.payload]]
arch = "x86_64"
reference = "{PIN}"
built_at_s = {NOW_UNIX}
"#
    ))
    .unwrap_or_else(|e| panic!("{e}"))
}

/// One source, one unit kind, a test of its own.
pub(crate) struct Bed {
    pub slots: Slots,
    pub clock: ManualClock,
    pub dir: TempDir,
}

pub(crate) struct KindSpec {
    pub name: &'static str,
    pub slots: u32,
    pub capability: Capability,
    pub command: Vec<String>,
}

pub(crate) fn sh(script: &str) -> Vec<String> {
    vec!["sh".into(), "-c".into(), script.into()]
}

impl Bed {
    /// `sources`: the name, the tier, the most units, and the script lines.
    pub fn new(cfg: Config, sources: Vec<(&str, &str, Option<u32>, Vec<String>)>, kinds: Vec<KindSpec>) -> Bed {
        let dir = TempDir::new();
        Bed::in_dir(cfg, sources, kinds, dir, ManualClock::default())
    }

    pub fn in_dir(
        mut cfg: Config,
        sources: Vec<(&str, &str, Option<u32>, Vec<String>)>,
        kinds: Vec<KindSpec>,
        dir: TempDir,
        clock: ManualClock,
    ) -> Bed {
        let mut srcs = Vec::new();
        for (name, tier, max, lines) in sources {
            let sc: crate::config::SourceConfig = serde_json::from_value(json!({
                "name": name, "kind": "fake", "tier": tier, "arch": "x86_64", "operator_controlled": true, "slots": max
            }))
            .unwrap();
            cfg.source.push(sc.clone());
            let script = parse_script(&lines.join("\n")).unwrap();
            srcs.push(Source {
                cfg: sc,
                source: Box::new(FakeSource::new(name, Arc::new(clock.clone()), script)),
                revoked: false,
            });
        }
        let mut ks = Vec::new();
        for k in kinds {
            let unit_cfg: crate::config::UnitConfig =
                serde_json::from_value(json!({ "slots": k.slots, "command": k.command })).unwrap();
            cfg.unit.insert(k.name.into(), unit_cfg.clone());
            let driver = FakeUnit::new(&dir.0.join(k.name), k.capability.clone(), "linux/amd64");
            ks.push(UnitKind {
                name: k.name.into(),
                cfg: unit_cfg,
                driver: Box::new(driver),
                capability: k.capability,
            });
        }
        let slots =
            Slots::start(cfg, &dir.0.join("state"), Arc::new(clock.clone()), Box::new(|| NOW_UNIX), srcs, ks).unwrap();
        Bed { slots, clock, dir }
    }

    /// One tick: the clock moves by one second.
    pub fn tick(&mut self) {
        self.slots.tick();
        self.clock.advance(Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(5));
    }

    /// Tick until `done` holds, up to `max` ticks; give the ticks.
    pub fn until(&mut self, max: u32, done: impl Fn(&Slots) -> bool) -> u32 {
        for n in 0..max {
            if done(&self.slots) {
                return n;
            }
            self.tick();
        }
        panic!("not done after {max} ticks: {:#?}", self.states());
    }

    pub fn states(&self) -> Vec<(String, State)> {
        self.slots.jobs().map(|j| (j.id.to_string(), j.state)).collect()
    }

    pub fn state(&self, job: &str) -> Option<State> {
        self.slots.jobs().find(|j| j.id.as_str() == job).map(|j| j.state)
    }

    pub fn all_final(s: &Slots) -> bool {
        s.jobs().all(|j| j.state.is_final())
    }
}
