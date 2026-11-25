//! The cache of each job of the slots (T-480): the grant at the start, the
//! measure while a `rw` job runs, and the end. Each walk runs on the
//! thread of the caches ([`super::Worker`]); the tick only sends and
//! reads.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use podrunner_schema::ExitReason;
use podrunner_schema::offer::TrustInputs;
use podrunner_unit::Signal;

use super::worker::{Done, Task};
use super::{Grant, Key, MOUNT, Mode, decide, scope, store};
use crate::audit;
use crate::engine::Slots;
use crate::state::State;

impl Slots {
    /// The cache of a placed job at its start: its mount, with its
    /// variables in `env`. None when the job gets no cache.
    pub(crate) fn cache_start(&mut self, key: &Key, env: &mut BTreeMap<String, String>) -> Vec<(PathBuf, String)> {
        let (Some(cfg), Some(w), Some(j)) = (&self.cfg.cache, &self.cache_worker, self.jobs.get(key)) else {
            return Vec::new();
        };
        let (Some(offer), Some(p), Some(unit)) = (&j.offer, &j.placement, &j.unit) else { return Vec::new() };
        let mounts = self.kinds[p.kind].driver.host_mounts();
        let (mut mode, k) = match decide(cfg, p.tier, offer, mounts) {
            Ok(d) => d,
            Err(why) => {
                eprintln!("podrunner: {unit}: no cache: {why}");
                return Vec::new();
            }
        };
        let (id, text) = scope(&self.sources[key.0].cfg.name, offer, p.tier, &k);
        // One writer of a cache at a time: the bytes of a job are its own.
        if mode == Mode::Rw && self.cache_in_use(key).any(|(i, m)| i == id && m == Mode::Rw) {
            mode = Mode::Ro;
        }
        let root = w.root.clone();
        let (data, since) = match store::open(&root, &id, &text, mode, cfg.env.values().map(String::as_str)) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("podrunner: {unit}: no cache: {}: {e}", root.display());
                return Vec::new();
            }
        };
        eprintln!("podrunner: {unit}: the cache {id} ({}), {}", mode.as_str(), text.trim_end());
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
        let grant = Grant { id, root, data: data.clone(), mode, since, written: 0 };
        if let Some(j) = self.jobs.get_mut(key) {
            j.times.cache = Some(grant);
        }
        vec![(data, target)]
    }

    /// The caches of the jobs other than `key` that have not ended. A job
    /// whose end the thread measures still holds its cache.
    fn cache_in_use<'a>(&'a self, key: &'a Key) -> impl Iterator<Item = (String, Mode)> + 'a {
        let open = self.jobs.iter().filter(move |(k, j)| *k != key && !j.state.is_final());
        open.filter_map(|(_, j)| j.times.cache.as_ref().map(|g| (g.id.clone(), g.mode)))
    }

    /// Whether the cache of the job `key` holds for the real job of
    /// `facts` that the forge gave its runner (T-493).
    pub(crate) fn cache_holds(&self, key: &Key, facts: &TrustInputs) -> bool {
        let (Some(cfg), Some(j)) = (&self.cfg.cache, self.jobs.get(key)) else { return true };
        let (Some(g), Some(offer), Some(p)) = (&j.times.cache, &j.offer, &j.placement) else { return true };
        let mounts = self.kinds[p.kind].driver.host_mounts();
        super::holds(cfg, g, &self.sources[key.0].cfg.name, offer, facts, p.tier, mounts)
    }

    /// After the destroy of the unit of `key`: give the thread the measure
    /// of a `rw` cache and the bound. True when the report waits for it.
    pub(crate) fn cache_end(&mut self, key: &Key) -> bool {
        let Some(cfg) = self.cfg.cache.clone() else { return false };
        let Some(g) = self.jobs.get(key).and_then(|j| j.times.cache.clone()) else { return false };
        if g.mode != Mode::Rw {
            return false;
        }
        let used: BTreeSet<String> = self.cache_in_use(key).map(|(id, _)| id).collect();
        let Some(w) = self.cache_worker.as_mut() else { return false };
        let task =
            Task::End { key: key.clone(), grant: g, cfg: Box::new(cfg), used, now: std::time::SystemTime::now() };
        if !w.send(task) {
            return false;
        }
        w.ending.insert(key.clone());
        true
    }

    /// Each tick: read the results of the thread, and give it the measure
    /// of each running `rw` cache every [`crate::disk::CHECK_MS`].
    pub(crate) fn cache_tick(&mut self, now: u64) {
        let Some(w) = self.cache_worker.as_ref() else { return };
        for done in w.done() {
            match done {
                Done::Size { key, bytes } => self.cache_sized(&key, bytes, now),
                Done::End { key, written, gone } => self.cache_ended(&key, written, gone, now),
            }
        }
        let Some(max) = self.cfg.cache.as_ref().map(|c| c.max_bytes) else { return };
        let Some(w) = self.cache_worker.as_mut() else { return };
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

    /// The thread measured the end of the job `key`: its bytes go to the
    /// report, and each cache to remove that no job took since goes to the
    /// trash.
    fn cache_ended(&mut self, key: &Key, written: u64, gone: Vec<(String, &'static str)>, now: u64) {
        if let Some(c) = self.jobs.get_mut(key).and_then(|j| j.times.cache.as_mut()) {
            c.written = written;
        }
        let used: BTreeSet<String> = self.cache_in_use(key).map(|(id, _)| id).collect();
        if let Some(w) = self.cache_worker.as_mut() {
            w.ending.remove(key);
            for (id, why) in gone.into_iter().filter(|(id, _)| !used.contains(id)) {
                match store::evict(&w.root, &id) {
                    Ok(trash) => {
                        eprintln!("podrunner: the cache {id} is removed: {why}");
                        let _ = w.send(Task::Remove(trash));
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
