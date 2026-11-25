//! The cache of each job of the slots (T-480): the grant at the start, the
//! measure while a `rw` job runs, and the end. Each walk runs on the
//! thread of the caches ([`super::Worker`]); the tick only sends and
//! reads.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use podrunner_schema::ExitReason;
use podrunner_schema::offer::TrustInputs;
use podrunner_source::RealJob;
use podrunner_unit::Signal;

use super::worker::{Done, Task};
use super::{Grant, Key, MOUNT, Mode, Mounts, NO_RO_MOUNT, NoCache, decide, scope, store};
use crate::audit;
use crate::engine::Slots;
use crate::state::State;

/// Whether the payload of a unit is known to carry the job-started hook of
/// the runner image. No pin and no kind says so (T-663): on a unit with no
/// hook, a step asks first with the ref and the repository id that it
/// chooses. So a job of another run gets no `rw` cache back (T-653,
/// `docs/decisions.md`).
const PAYLOAD_HOOK_KNOWN: bool = false;

impl Slots {
    /// The cache of a placed job at its start: its mount, with its
    /// variables in `env`. None when the job gets no cache.
    pub(crate) fn cache_start(&mut self, key: &Key, env: &mut BTreeMap<String, String>) -> Vec<(PathBuf, String)> {
        let (Some(cfg), Some(_), Some(j)) = (&self.cfg.cache, &self.cache_worker, self.jobs.get(key)) else {
            return Vec::new();
        };
        let (Some(offer), Some(p), Some(unit)) = (&j.offer, &j.placement, &j.unit) else { return Vec::new() };
        let (cfg, offer, unit) = (cfg.clone(), offer.clone(), unit.clone());
        let mounts = self.cache_mounts(p.kind);
        let mut decided = decide(&cfg, p.tier, &offer, mounts);
        let tier = p.tier;
        if let Ok((mode, k)) = &mut decided {
            let id = scope(&self.sources[key.0].cfg.name, &offer, tier, k).0;
            // One writer of a cache at a time: the bytes of a job are its own.
            if *mode == Mode::Rw && self.cache_writer(key, &id) {
                *mode = Mode::Ro;
                if !mounts.ro {
                    decided = Err(NO_RO_MOUNT);
                }
            }
        }
        let (mode, k) = match decided {
            Ok(d) => d,
            Err(NoCache { code, why }) => {
                eprintln!("podrunner: {unit}: no cache: {why}");
                self.audit(audit::Kind::Cache, key.0, &key.1, None, Some(format!("none {code}")));
                return Vec::new();
            }
        };
        let (id, text) = scope(&self.sources[key.0].cfg.name, &offer, tier, &k);
        let Some(w) = self.cache_worker.as_ref() else { return Vec::new() };
        let root = w.root.clone();
        // The thread writes no count of a scope that a job took.
        w.uses().take(&id);
        let (data, since) = match store::open(&root, &id, &text, mode, cfg.env.values().map(String::as_str)) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("podrunner: {unit}: no cache: {}: {e}", root.display());
                self.audit(audit::Kind::Cache, key.0, &key.1, None, Some("none open-failed".into()));
                return Vec::new();
            }
        };
        eprintln!("podrunner: {unit}: the cache {id} ({}), {}", mode.as_str(), text.trim_end());
        self.audit(audit::Kind::Cache, key.0, &key.1, None, Some(mode.as_str().into()));
        env.insert("PODRUNNER_CACHE".into(), MOUNT.into());
        env.insert("PODRUNNER_CACHE_MODE".into(), mode.as_str().into());
        // A tool writes in the directory of its variable: in a `ro` cache
        // each write fails, so a `ro` job gets none (T-650 gives it a
        // layer to write). A variable that podrunner set before (the CA
        // bundle of the broker) stays.
        if mode == Mode::Rw {
            for (name, path) in &cfg.env {
                env.entry(name.clone()).or_insert_with(|| format!("{MOUNT}/{path}"));
            }
        }
        let target = if mode == Mode::Ro { format!("{MOUNT}:ro") } else { MOUNT.to_string() };
        let written = (mode == Mode::Ro).then_some(0);
        let grant = Grant { id, root, data: data.clone(), mode, since, written };
        if let Some(j) = self.jobs.get_mut(key) {
            j.times.cache = Some(grant);
        }
        vec![(data, target)]
    }

    /// What the kind `kind` proves of a host directory.
    fn cache_mounts(&self, kind: usize) -> Mounts {
        let k = &self.kinds[kind];
        Mounts::of(k.driver.as_ref(), &k.capability)
    }

    /// Whether a job other than `key` writes the cache `id` now, or its
    /// count after its end runs: the count of a job holds only its own
    /// writes.
    fn cache_writer(&self, key: &Key, id: &str) -> bool {
        let counting = self.cache_worker.as_ref().is_some_and(|w| w.ending.iter().any(|(k, s)| k != key && s == id));
        counting
            || self.jobs.iter().any(|(k, j)| {
                k != key
                    && !j.state.is_final()
                    && j.times.cache.as_ref().is_some_and(|g| g.id == id && g.mode == Mode::Rw)
            })
    }

    /// The holders of each scope, other than `but`: each job that has not
    /// ended with a cache, and each job whose count is not back.
    fn cache_holders(&self, but: Option<&Key>) -> BTreeMap<String, u32> {
        let mut held = BTreeMap::<String, u32>::new();
        let ending = self.cache_worker.as_ref().map(|w| &w.ending);
        for (k, j) in &self.jobs {
            let counted = ending.is_some_and(|e| e.contains_key(k));
            if Some(k) == but || counted || j.state.is_final() {
                continue;
            }
            if let Some(g) = &j.times.cache {
                *held.entry(g.id.clone()).or_default() += 1;
            }
        }
        for (k, id) in ending.into_iter().flatten() {
            if Some(k) != but {
                *held.entry(id.clone()).or_default() += 1;
            }
        }
        held
    }

    /// Whether the cache of the job `key` holds for the real job `real` of
    /// `facts` that the forge gave its runner (T-493).
    pub(crate) fn cache_holds(&self, key: &Key, real: &RealJob, facts: &TrustInputs) -> bool {
        let (Some(cfg), Some(j)) = (&self.cfg.cache, self.jobs.get(key)) else { return true };
        let (Some(g), Some(offer), Some(p)) = (&j.times.cache, &j.offer, &j.placement) else { return true };
        let mounts = self.cache_mounts(p.kind);
        let source = &self.sources[key.0].cfg.name;
        super::holds(cfg, g, source, offer, real, facts, p.tier, mounts, PAYLOAD_HOOK_KNOWN)
    }

    /// After the destroy of the unit of `key`: give the thread the count
    /// of a `rw` cache and the bound. True when the report waits for it,
    /// at most `[deadlines] report_s`; the job holds no slot meanwhile.
    pub(crate) fn cache_end(&mut self, key: &Key) -> bool {
        let Some(cfg) = self.cfg.cache.clone() else { return false };
        let Some(g) = self.jobs.get(key).and_then(|j| j.times.cache.clone()) else { return false };
        let Some(w) = self.cache_worker.as_mut().filter(|_| g.mode == Mode::Rw) else { return false };
        let (id, stamp) = (g.id.clone(), w.uses().stamp(&g.id));
        let now = std::time::SystemTime::now();
        if !w.send(Task::End { key: key.clone(), grant: g, cfg: Box::new(cfg), stamp, now }) {
            return false;
        }
        w.ending.insert(key.clone(), id);
        true
    }

    /// Each tick: read the results of the thread, report each job whose
    /// count did not come in `report_s`, and give the thread the measure
    /// of each running `rw` cache every [`crate::disk::CHECK_MS`].
    pub(crate) fn cache_tick(&mut self, now: u64) {
        let Some(w) = self.cache_worker.as_ref() else { return };
        for done in w.done() {
            match done {
                Done::Size { key, bytes } => self.cache_sized(&key, bytes, now),
                Done::End { key, written, gone } => self.cache_ended(&key, written, gone, now),
            }
        }
        let wait = self.cfg.deadlines.report_s.saturating_mul(1000);
        let late: Vec<Key> = self
            .jobs
            .iter()
            .filter(|(k, j)| {
                j.state == State::Destroyed
                    && now.saturating_sub(j.since_ms) >= wait
                    && self.cache_worker.as_ref().is_some_and(|w| w.ending.contains_key(*k))
            })
            .map(|(k, _)| k.clone())
            .collect();
        for key in late {
            eprintln!("podrunner: {}: the count of the cache is late: the report says unknown", key.1);
            self.reported(&key, now);
        }
        let held = self.cache_holders(None);
        let Some(max) = self.cfg.cache.as_ref().map(|c| c.max_bytes) else { return };
        let Some(w) = self.cache_worker.as_mut() else { return };
        w.uses().set_held(held);
        if now < w.next_ms {
            return;
        }
        w.next_ms = now + crate::disk::CHECK_MS;
        for (key, j) in &self.jobs {
            let live = matches!(j.state, State::Started | State::Running) && j.term_ms.is_none();
            let Some(g) = j.times.cache.as_ref().filter(|g| live && g.mode == Mode::Rw) else { continue };
            if !w.sizing.contains(key) && w.send(Task::Size { key: key.clone(), data: g.data.clone(), max }) {
                w.sizing.insert(key.clone());
            }
        }
    }

    /// A running `rw` job whose cache is above `max_bytes` ends with
    /// `limit`: the cache shares the file system of the state.
    fn cache_sized(&mut self, key: &Key, bytes: u64, now: u64) {
        if let Some(w) = self.cache_worker.as_mut() {
            w.sizing.remove(key);
        }
        let max = self.cfg.cache.as_ref().map_or(u64::MAX, |c| c.max_bytes);
        let live = self.jobs.get(key).is_some_and(|j| matches!(j.state, State::Started | State::Running));
        if live && bytes > max {
            self.audit(audit::Kind::Limit, key.0, &key.1, None, Some("cache".into()));
            self.end(key, ExitReason::Limit, Signal::Term, now);
        }
    }

    /// The thread counted the end of the job `key`: its bytes go to the
    /// report, and each cache to remove that no job holds goes to the
    /// trash.
    fn cache_ended(&mut self, key: &Key, written: u64, gone: Vec<(String, &'static str)>, now: u64) {
        if let Some(c) = self.jobs.get_mut(key).and_then(|j| j.times.cache.as_mut()) {
            c.written = Some(written);
        }
        let used: BTreeSet<String> = self.cache_holders(Some(key)).into_keys().collect();
        if let Some(w) = self.cache_worker.as_mut() {
            w.ending.remove(key);
            for (id, why) in gone.into_iter().filter(|(id, _)| !used.contains(id)) {
                match store::evict(&w.root, &id) {
                    Ok(trash) => {
                        eprintln!("podrunner: the cache {id} is removed: {why}");
                        w.remove(trash);
                    }
                    Err(e) => eprintln!("podrunner: the cache {id} stays: {e}"),
                }
            }
        }
        if self.jobs.get(key).is_some_and(|j| j.state == State::Destroyed) {
            self.reported(key, now);
        }
    }
}
