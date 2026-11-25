//! T-487: why a warm unit ends, the backoff of a row, a reserved unit that
//! only `failed` and `boot-timeout` end (P2-1), and each failure of a start
//! after the claim at the source, which destroys the unit (P2-2, P2-3).

use std::sync::Arc;

use podrunner_schema::{ContainerRoute, RefusalCode};
use serde_json::json;

use std::sync::atomic::Ordering;

use super::warm::{Gate, Seen, audit_details, confined, ready, warm_bed, warm_ids};
use super::*;
use crate::EgressPort;
use crate::audit::Kind as A;

fn ends(s: &Slots) -> Vec<String> {
    audit_details(s, A::Warm).into_iter().filter(|d| d.starts_with("end ")).collect()
}

/// Age, a new pin, a stale pin, a change of the record of the kind, a
/// smaller row and memory pressure each end a free warm unit.
#[test]
fn each_reason_ends_a_free_warm_unit() {
    type Change = fn(&mut Slots);
    let cases: [(&str, Change); 6] = [
        ("age", |s| s.kinds[0].cfg.warm_max_age_s = Some(1)),
        ("pin", |s| {
            let mut p = s.cfg.pin.payload[0].clone();
            p.reference = format!("new@sha256:{}", "3".repeat(64));
            s.cfg.pin.payload.insert(0, p);
        }),
        ("stale", |s| s.cfg.pin.payload[0].built_at_s = 1),
        ("capability", |s| s.kinds[0].capability.boundary = podrunner_schema::Boundary::Redirect),
        ("config", |s| s.kinds[0].cfg.warm = Some(crate::config::WarmCount::Count(0))),
        ("pressure", |s| s.warm.mem_probe = Box::new(|| Some(1))),
    ];
    for (why, change) in cases {
        let mut bed = warm_bed(2, 1, vec![], "");
        bed.until(4, |s| ready(s) == 1);
        let u = warm_ids(&bed.slots)[0].clone();
        change(&mut bed.slots);
        bed.tick();
        bed.tick();
        assert!(!warm_ids(&bed.slots).contains(&u), "{why}: the unit ended");
        assert_eq!(ends(&bed.slots).first().map(String::as_str), Some(format!("end reason={why}").as_str()), "{why}");
        assert!(bed.slots.kinds[0].driver.observe(&u).is_err(), "{why}: destroyed");
    }
}

/// Retesting a kind replaces its old free units with units of the new capability.
#[test]
fn a_capability_change_keeps_the_replacement_warm_unit() {
    let mut bed = warm_bed(2, 1, vec![], "");
    bed.until(4, |s| ready(s) == 1);
    let first = warm_ids(&bed.slots);
    bed.slots.kinds[0].capability.boundary = podrunner_schema::Boundary::Redirect;
    bed.tick();
    bed.until(8, |s| ready(s) == 1);
    let replacement = warm_ids(&bed.slots);
    assert_ne!(replacement, first);
    bed.tick();
    assert_eq!(warm_ids(&bed.slots), replacement);
}

/// A failed row waits without holding back another row of the same kind.
#[test]
fn a_row_in_backoff_does_not_hold_back_other_labels() {
    let mut bed = warm_bed(2, 1, vec![], "");
    bed.slots.kinds[0].cfg.warm =
        Some(crate::config::WarmCount::ByLabel([("*".to_string(), 1), ("cpu".to_string(), 1)].into()));
    bed.slots.cfg.pin.payload[0].labels = vec!["cpu".into()];
    bed.tick();
    assert!(bed.slots.warm.backoff.contains_key(&(0, "*".into())));
    assert!(bed.slots.status().kinds[0].warm.as_ref().unwrap().paused_reason.is_none());
    bed.until(8, |s| ready(s) == 1);
    assert_eq!(bed.slots.warm.units.values().next().unwrap().row, "cpu");
}

/// The whole unsigned range of the configuration has a defined age in ms.
#[test]
fn a_large_warm_max_age_does_not_wrap_in_milliseconds() {
    let mut bed = warm_bed(2, 1, vec![], "");
    bed.slots.kinds[0].cfg.warm_max_age_s = Some(u64::MAX);
    bed.until(4, |s| ready(s) == 1);
    let unit = warm_ids(&bed.slots);
    bed.tick();
    assert_eq!(warm_ids(&bed.slots), unit);
}

/// A unit whose top process ended is destroyed, and its row backs off.
#[test]
fn a_warm_unit_that_ended_is_destroyed_and_the_row_backs_off() {
    let mut bed = warm_bed(2, 1, vec![], "");
    bed.until(4, |s| ready(s) == 1);
    let u = warm_ids(&bed.slots)[0].clone();
    bed.slots.kinds[0].driver.signal(&u, podrunner_unit::Signal::Kill).unwrap();
    bed.until(10, |s| !s.warm.units.contains_key(&u));
    assert_eq!(ends(&bed.slots), ["end reason=failed"]);
    assert!(bed.slots.kinds[0].driver.observe(&u).is_err(), "destroyed");
    assert_eq!(bed.slots.warm.backoff[&(0, "*".to_string())].0, 1);
    for _ in 0..30 {
        bed.tick();
    }
    assert_eq!(bed.slots.warm_units(), 0, "the row waits 60 s");
}

/// Three failures in a row (here: no payload for the row) wait 60 s, then
/// 120 s, then pause the row for one hour.
#[test]
fn three_failures_in_a_row_pause_the_row() {
    let mut bed = warm_bed(2, 1, vec![], "");
    bed.slots.cfg.pin.payload[0].arch = "aarch64".into();
    let fails = |b: &Bed| b.slots.warm.backoff.get(&(0, "*".to_string())).map_or(0, |x| x.0);
    let mut seen = Vec::new();
    for _ in 0..400 {
        bed.tick();
        seen.push(fails(&bed));
    }
    let at = |n: u32| seen.iter().position(|f| *f == n).unwrap();
    assert_eq!((at(1), at(2), at(3)), (0, 60, 180), "{seen:?}");
    assert_eq!(fails(&bed), 3, "paused for one hour");
    assert_eq!(bed.slots.warm_units(), 0);
}

/// P2-6: a stale pin gives no payload: no unit, and the row backs off; it
/// does not boot a unit on each tick.
#[test]
fn a_stale_pin_does_not_boot_a_unit_each_tick() {
    let mut bed = warm_bed(2, 1, vec![], "");
    bed.slots.cfg.pin.payload[0].built_at_s = 1;
    for _ in 0..30 {
        bed.tick();
    }
    assert_eq!(bed.slots.warm_units(), 0);
    let starts = audit_details(&bed.slots, A::Warm);
    assert!(starts.is_empty(), "no unit: {starts:?}");
    assert_eq!(bed.slots.warm.backoff[&(0, "*".to_string())].0, 1, "one try in 30 s");
}

/// A warm unit that does not get ready in `[deadlines] start_s` ends.
#[test]
fn a_warm_unit_that_is_not_ready_in_time_ends() {
    let mut bed = warm_bed(2, 1, vec![], "[deadlines]\nstart_s = 2");
    bed.tick();
    let u = warm_ids(&bed.slots)[0].clone();
    // The waiter never marks the unit ready: its mark is a directory.
    let root = bed.slots.kinds[0].driver.observe(&u).unwrap().unit_root.unwrap();
    let _ = std::fs::remove_file(root.join(".podrunner-warm"));
    std::fs::create_dir_all(root.join(".podrunner-warm")).unwrap();
    bed.slots.warm.units.get_mut(&u).unwrap().ready_ms = None;
    for _ in 0..4 {
        bed.tick();
    }
    assert!(!bed.slots.warm.units.contains_key(&u));
    assert_eq!(ends(&bed.slots), ["end reason=boot-timeout"]);
}

/// A warm unit that leaves a residue makes the host `degraded`, as a job
/// unit does.
#[test]
fn a_warm_residue_makes_the_host_degraded() {
    let mut bed = warm_bed(2, 1, vec![], "");
    let plant = podrunner_fake::FakeUnit::new(&bed.dir.0.join("leak"), confined(), "linux/amd64")
        .with_plant(podrunner_fake::UnitPlant::KeepFiles);
    bed.slots.kinds[0].driver = Arc::new(plant);
    bed.until(4, |s| ready(s) == 1);
    bed.slots.drain();
    bed.tick();
    assert!(bed.slots.degraded().is_some_and(|d| d.contains("files")), "{:?}", bed.slots.degraded());
}

/// P2-1: while the mint of its job waits, a reserved warm unit stays
/// through age, a new pin, a change of the record, a smaller row, memory
/// pressure, a drain and an evict; the job then takes it. (A stale pin
/// ends the job itself at its mint.)
#[test]
fn a_reserved_warm_unit_ends_only_by_failure() {
    let mut bed = warm_bed(2, 1, vec![offer(5, "j-1", json!({}))], "");
    let (_, mint) = Gate::over(&mut bed);
    mint.store(true, Ordering::SeqCst);
    bed.until(4, |s| ready(s) == 1);
    let u = warm_ids(&bed.slots)[0].clone();
    bed.until(10, |s| s.warm.units.values().any(|w| w.reserved.is_some()));
    bed.slots.kinds[0].cfg.warm_max_age_s = Some(1);
    let mut p = bed.slots.cfg.pin.payload[0].clone();
    p.reference = format!("new@sha256:{}", "3".repeat(64));
    bed.slots.cfg.pin.payload.insert(0, p);
    bed.slots.kinds[0].cfg.warm = Some(crate::config::WarmCount::Count(0));
    bed.slots.kinds[0].capability.boundary = podrunner_schema::Boundary::Guest;
    bed.slots.warm.mem_probe = Box::new(|| Some(1));
    bed.slots.warm_make_way(0, 0);
    bed.slots.drain();
    bed.tick();
    bed.tick();
    assert!(bed.slots.warm.units.contains_key(&u), "reserved: {:?}", ends(&bed.slots));
    mint.store(false, Ordering::SeqCst);
    bed.until(30, |s| s.jobs().any(|j| j.state == State::Started));
    let j = bed.slots.jobs().next().unwrap();
    assert_eq!((j.unit.as_ref(), j.warm), (Some(&u), true), "the job took its reserved unit");
    assert!(ends(&bed.slots).is_empty(), "{:?}", ends(&bed.slots));
}

/// P2-1: a reserved warm unit that fails before the take leaves its job a
/// cold unit.
#[test]
fn a_reserved_warm_unit_that_fails_gives_its_job_a_cold_unit() {
    let mut bed = warm_bed(2, 1, vec![offer(5, "j-1", json!({}))], "");
    let (_, mint) = Gate::over(&mut bed);
    mint.store(true, Ordering::SeqCst);
    bed.until(4, |s| ready(s) == 1);
    let u = warm_ids(&bed.slots)[0].clone();
    bed.until(10, |s| s.warm.units.values().any(|w| w.reserved.is_some()));
    bed.slots.kinds[0].driver.signal(&u, podrunner_unit::Signal::Kill).unwrap();
    bed.until(10, |s| !s.warm.units.contains_key(&u));
    mint.store(false, Ordering::SeqCst);
    bed.until(30, |s| s.jobs().any(|j| j.state == State::Started));
    let j = bed.slots.jobs().next().unwrap();
    assert!(!j.warm && j.unit.as_ref() != Some(&u), "a cold unit");
    assert_eq!(ends(&bed.slots)[0], "end reason=failed");
}

/// An egress port whose open fails.
struct NoOpen(Seen);

impl EgressPort for NoOpen {
    fn open(&self, _: &str, _: &str, _: &std::path::Path) -> Result<usize, String> {
        Err("no".into())
    }
    fn hold(&self, unit: &str, socket: &std::path::Path) -> Result<(), String> {
        self.0.hold(unit, socket)
    }
    fn close(&self, unit: &str) {
        self.0.close(unit)
    }
    fn alive(&self) -> bool {
        true
    }
}

/// Hooks whose open fails.
struct NoHooks;

impl crate::hooks::HookPort for NoHooks {
    fn open(
        &self,
        _: &str,
        _: Arc<dyn podrunner_unit::UnitDriver>,
        _: &podrunner_schema::UnitId,
        _: &std::path::Path,
        _: Option<&str>,
        _: Option<crate::hooks::ContainerCa<'_>>,
    ) -> Result<std::collections::BTreeMap<String, String>, String> {
        Err("no".into())
    }
    fn close(&self, _: &podrunner_schema::UnitId) {}
    fn take(&self, _: &podrunner_schema::UnitId) -> (Vec<RefusalCode>, Vec<crate::hooks::HookEvent>) {
        Default::default()
    }
}

/// P2-3: each start that fails after the claim at the source (the socket
/// directory, a stale pin, the open of the egress, the hooks) destroys the
/// warm unit; none goes back to the pool.
#[test]
fn each_failure_of_a_start_destroys_the_warm_unit() {
    type Break = fn(&mut Bed, &podrunner_schema::UnitId);
    let cases: [(&str, Break, RefusalCode); 4] = [
        (
            "socket dir",
            |b, u| {
                let dir = crate::start::socket_dir(&b.slots.state_dir, u);
                std::fs::remove_dir_all(&dir).unwrap();
                std::fs::write(&dir, "").unwrap();
            },
            RefusalCode::UnitStartFailed,
        ),
        ("stale", |b, _| b.slots.cfg.pin.payload[0].max_age_s = Some(1), RefusalCode::ImageStale),
        (
            "egress",
            |b, _| {
                let old = b.slots.egress.take();
                drop(old);
                b.slots.egress = Some(Box::new(NoOpen(Seen::default())));
            },
            RefusalCode::UnitStartFailed,
        ),
        (
            "hooks",
            |b, _| {
                b.slots.kinds[0].capability.container = ContainerRoute::Hooks;
                b.slots.set_hooks(Box::new(NoHooks));
            },
            RefusalCode::UnitStartFailed,
        ),
    ];
    for (what, brk, code) in cases {
        let mut bed = warm_bed(2, 1, vec![offer(6, "j-1", json!({}))], "");
        let (_, mint) = Gate::over(&mut bed);
        mint.store(true, Ordering::SeqCst);
        bed.until(5, |s| ready(s) == 1);
        let u = warm_ids(&bed.slots)[0].clone();
        bed.until(10, |s| s.warm.units.values().any(|w| w.reserved.is_some()));
        brk(&mut bed, &u);
        mint.store(false, Ordering::SeqCst);
        bed.until(10, |s| s.jobs().count() == 1 && Bed::all_final(s));
        let j = bed.slots.jobs().next().unwrap();
        assert_eq!((j.state, j.refusal), (State::Failed, Some(code)), "{what}");
        assert!(bed.slots.kinds[0].driver.observe(&u).is_err(), "{what}: destroyed");
        assert!(!bed.slots.warm.units.values().any(|w| w.reserved.is_some()), "{what}");
    }
}

/// With no warm unit, the pool reads no memory probe.
#[test]
fn no_warm_unit_reads_no_memory_probe() {
    let mut bed = warm_bed(2, 0, vec![], "");
    let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let r = reads.clone();
    bed.slots.warm.mem_probe = Box::new(move || {
        r.fetch_add(1, Ordering::SeqCst);
        Some(u64::MAX)
    });
    for _ in 0..3 {
        bed.tick();
    }
    assert_eq!(reads.load(Ordering::SeqCst), 0);
}

/// A paused row refills on the next tick when the operator pins a new
/// payload for it.
#[test]
fn a_new_pin_ends_the_backoff_of_a_row() {
    let mut bed = warm_bed(2, 1, vec![], "");
    bed.slots.cfg.pin.payload[0].arch = "aarch64".into();
    bed.until(400, |s| s.warm.backoff.get(&(0, "*".to_string())).is_some_and(|b| b.0 == 3));
    let mut p = bed.slots.cfg.pin.payload[0].clone();
    p.arch = "x86_64".into();
    p.reference = format!("new@sha256:{}", "3".repeat(64));
    bed.slots.cfg.pin.payload.push(p);
    bed.tick();
    assert_eq!(bed.slots.warm_units(), 1, "the row boots with the new pin");
}
