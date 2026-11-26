//! The take of a warm unit by a job (T-487): placement gives the kind; a
//! free warm unit of that kind with the payload of the job and the same
//! limits is reserved for it, else the job gets a cold unit and an audit
//! line `warm-miss`. The take itself comes in the mint, after each check
//! that can fail (P2-3).

use podrunner_schema::{JobId, JobOffer, UnitId};
use podrunner_unit::limits::Limits;

use crate::audit;
use crate::engine::Slots;
use crate::placement::Placement;
use crate::state::State;
use crate::warm::End;

type Key = (usize, JobId);

impl Slots {
    /// The unit of the placed job `key`: a free warm unit that fits it
    /// (reserved now), or a new id. Before a cold unit, the oldest free
    /// warm unit that holds its slot or its unit of `max_units` ends.
    pub(crate) fn unit_for(&mut self, key: &Key, p: &Placement, offer: &JobOffer, now: u64) -> UnitId {
        let kind = p.kind;
        let pool = !self.kinds[kind].cfg.warm_rows().is_empty();
        let k = &self.kinds[kind];
        let applied = k.driver.applied_limits(&Limits::of_offer(&offer.limits));
        // A job with a cache needs `start`: a claim has no mount. A job
        // that gets no cache takes a warm unit (T-648).
        let mount = self.cache_wanted(kind, p.tier, offer);
        let mut free: Vec<(&UnitId, &crate::warm::Warm)> =
            self.warm.units.iter().filter(|(_, w)| w.kind == kind && w.reserved.is_none()).collect();
        // A ready unit first, then the oldest.
        free.sort_by_key(|(_, w)| (w.ready_ms.is_none(), w.made_ms));
        let mut miss = "none";
        let mut pick = None;
        for (u, w) in free {
            if offer.sandbox.is_some() {
                miss = "sandbox";
            } else if mount {
                miss = "mount";
            } else if w.payload != p.payload {
                miss = "payload";
                continue;
            } else if w.limits != applied {
                miss = "limits";
                continue;
            } else {
                pick = Some(u.clone());
            }
            break;
        }
        if let Some(unit) = pick {
            let w = self.warm.units.get_mut(&unit).expect("a free warm unit");
            w.reserved = Some(key.clone());
            let waited = w.ready_ms.map_or(0, |r| now.saturating_sub(r).div_ceil(1000));
            let boot = w.ready_ms.map(|r| r.saturating_sub(w.made_ms).div_ceil(1000));
            let boot = boot.map_or_else(|| "booting".to_string(), |b| b.to_string());
            let detail = format!("unit={unit} kind={} waited_s={waited} boot_s={boot}", self.kinds[kind].name);
            self.audit(audit::Kind::WarmTake, key.0, &key.1, None, Some(detail));
            return unit;
        }
        if pool {
            self.audit(audit::Kind::WarmMiss, key.0, &key.1, None, Some(format!("reason={miss}")));
        }
        self.make_room(kind, now);
        self.ids.next()
    }

    /// End the oldest free warm unit that holds the last slot of `kind`,
    /// or the last unit of `max_units`, so that a cold unit fits.
    fn make_room(&mut self, kind: usize, now: u64) {
        let views = self.kind_views();
        let free = |k: Option<usize>| {
            let mut v: Vec<(&UnitId, u64)> = self
                .warm
                .units
                .iter()
                .filter(|(_, w)| w.reserved.is_none() && k.is_none_or(|k| w.kind == k))
                .map(|(u, w)| (u, w.made_ms))
                .collect();
            v.sort_by_key(|(_, made)| *made);
            v.first().map(|(u, _)| (*u).clone())
        };
        if views[kind].running + self.warm_free(kind) >= self.kinds[kind].cfg.slots
            && let Some(u) = free(Some(kind))
        {
            return self.end_warm(&u, End::Evict, now);
        }
        let running: u32 = views.iter().map(|k| k.running).sum();
        let warm = self.warm.units.values().filter(|w| w.reserved.is_none()).count() as u32;
        if self.cfg.max_units.is_some_and(|m| running + warm >= m)
            && let Some(u) = free(None)
        {
            self.end_warm(&u, End::Evict, now);
        }
    }

    /// True when the unit of `key` is a warm unit reserved for it.
    pub(crate) fn reserved_warm(&self, key: &Key) -> Option<UnitId> {
        let unit = self.jobs.get(key)?.unit.clone()?;
        self.warm.units.get(&unit).filter(|w| w.reserved.as_ref() == Some(key)).map(|_| unit)
    }

    /// The take: the warm unit of `key` leaves the pool, and its `@warm`
    /// line is final. From here each failure destroys it with the job.
    pub(crate) fn take_warm(&mut self, key: &Key, now: u64) -> bool {
        let Some(unit) = self.reserved_warm(key) else { return false };
        let w = self.warm.units.remove(&unit).expect("a reserved warm unit");
        self.warm_line(&unit, w.kind, State::Released, now);
        self.audit_host(&unit, format!("taken job={}", key.1));
        if let Some(j) = self.jobs.get_mut(key) {
            j.warm = true;
            j.times.warm = Some(crate::report::WarmTimes { made_ms: w.made_ms, ready_ms: w.ready_ms, taken_ms: now });
        }
        true
    }

    /// The job of `key` ends before its take: its reserved warm unit ends
    /// too, since the source had the claim of its runner (P2-2).
    pub(crate) fn drop_reserved(&mut self, key: &Key, now: u64) {
        if let Some(unit) = self.reserved_warm(key) {
            self.end_warm(&unit, End::JobEnded, now);
        }
    }

    /// The claim of `key` was not sent to its source: its reserved warm
    /// unit goes back to the pool untouched.
    pub(crate) fn unreserve(&mut self, key: &Key) {
        if let Some(unit) = self.reserved_warm(key)
            && let Some(w) = self.warm.units.get_mut(&unit)
        {
            w.reserved = None;
        }
    }
}
