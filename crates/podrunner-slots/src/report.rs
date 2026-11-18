//! The run report of a job (`run-report.v1` of `pod-job`).

use podrunner_schema::{Assigned, ExitReason, JobId, Residual, RunReport, UnitFields, Verdict as AssignedVerdict};
use podrunner_unit::Count;

use crate::engine::Slots;

type Key = (usize, JobId);

pub(crate) fn residual(c: Count) -> Residual {
    match c {
        Count::Known(n) => Residual::Count(n),
        Count::Unknown => Residual::Unknown(podrunner_schema::report::Unknown::Unknown),
    }
}

/// The times and the measures of a job for its report (T-497).
#[derive(Debug, Clone, Default)]
pub struct Times {
    pub ended_ms: Option<u64>,
    pub destroyed_ms: Option<u64>,
    /// The most bytes under the root of the unit that a disk check read.
    pub disk_peak: Option<u64>,
    /// The bytes in and out of the connections of the unit through its
    /// egress module (the egress lines of T-486).
    pub net_rx: u64,
    pub net_tx: u64,
}

impl Times {
    /// The time of `state`, at its first time.
    pub fn mark(&mut self, state: crate::state::State, now: u64) {
        match state {
            crate::state::State::Ended => self.ended_ms = self.ended_ms.or(Some(now)),
            crate::state::State::Destroyed => self.destroyed_ms = self.destroyed_ms.or(Some(now)),
            _ => {}
        }
    }
}

/// The seconds from `a` to `b` (ms), rounded up, when both are known.
fn secs(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    let (a, b) = (a?, b?);
    Some(b.saturating_sub(a).div_ceil(1000))
}

/// A report of `job` of `source` on `host` with no fact yet: the exit
/// reason `lost`, and each optional field empty.
pub(crate) fn base(job_id: JobId, source: String, host: String) -> RunReport {
    RunReport {
        schema: podrunner_schema::SCHEMA,
        job_id,
        source,
        tier: None,
        host,
        exit_reason: ExitReason::Lost,
        exit_code: None,
        wall_s: 0,
        cpu_s: None,
        peak_rss_mib: None,
        claim_to_first_step_s: None,
        job_images: Vec::new(),
        refusal_code: None,
        unit: None,
        arch: None,
        assigned: None,
        image_digest: None,
        runner_version: None,
        prepare_s: None,
        start_s: None,
        run_s: None,
        destroy_s: None,
        disk_peak_bytes: None,
        net_rx_bytes: None,
        net_tx_bytes: None,
        cache_read_bytes: None,
        cache_written_bytes: None,
        unknown_reasons: Default::default(),
        log_tail: None,
    }
}

impl Slots {
    /// Build the report of a job, keep it, and send it to its source: the
    /// full report to a source that the operator controls, else the
    /// public one.
    pub(crate) fn report(&mut self, key: &Key) {
        let j = &self.jobs[key];
        let unit = match (&j.unit, &j.placement, j.residue) {
            (Some(u), Some(p), Some(r)) => {
                let k = &self.kinds[p.kind];
                Some(UnitFields {
                    unit_id: u.clone(),
                    unit_kind: k.name.clone(),
                    boundary: k.capability.boundary,
                    principal: k.capability.principal,
                    egress_hold: k.capability.egress_hold,
                    payload: podrunner_unit::parse_pinned(&p.payload).map(|(_, d)| d.to_string()).unwrap_or_default(),
                    residual_processes: residual(r.processes),
                    residual_files: residual(r.files),
                })
            }
            _ => None,
        };
        let unit_ran = unit.is_some();
        let mut r = base(key.1.clone(), self.sources[key.0].cfg.name.clone(), self.cfg.host.clone());
        r.tier = j.placement.as_ref().map(|p| p.tier);
        r.exit_reason = j.reason.unwrap_or(ExitReason::Lost);
        r.exit_code = j.exit_code;
        r.wall_s = self.now().saturating_sub(j.offered_ms) / 1000;
        r.refusal_code = j.refusal;
        r.unit = unit;
        r.claim_to_first_step_s = j.claimed_ms.zip(j.first_step_ms).map(|(c, f)| f.saturating_sub(c) / 1000);
        r.job_images = j.images.clone();
        // The use of the unit, when a unit ran (T-108).
        r.cpu_s = unit_ran.then(|| j.usage.report().0);
        r.peak_rss_mib = unit_ran.then(|| j.usage.report().1);
        // The architecture of the unit (T-514), and the real job that its
        // runner got when it was not the run of the offer (T-493).
        r.arch = j.placement.as_ref().map(|p| self.kinds[p.kind].capability.arch.clone());
        // The times and the measures (T-497): from the claim to the start
        // of the unit, to the first step, to the end of the unit, to its
        // destroy; the peak of the disk; the bytes through the egress.
        r.prepare_s = secs(j.claimed_ms, j.started_ms);
        r.start_s = secs(j.started_ms, j.first_step_ms);
        r.run_s = secs(j.first_step_ms.or(j.started_ms), j.times.ended_ms);
        r.destroy_s = secs(j.times.ended_ms, j.times.destroyed_ms);
        if let Some(p) = &j.placement {
            r.image_digest = podrunner_unit::parse_pinned(&p.payload).map(|(_, d)| d.to_string()).ok();
            let pinned = |x: &&crate::config::Payload| {
                x.reference == p.payload
                    || podrunner_unit::parse_pinned(&x.reference).is_ok_and(|(_, d)| d == p.payload)
            };
            r.runner_version = self.cfg.pin.payload.iter().find(pinned).and_then(|x| x.runner_version.clone());
        }
        if unit_ran && j.first_step_ms.is_none() {
            r.unknown_reasons.insert("start_s".into(), "the runner reached no first step".into());
        }
        if unit_ran {
            let unknown = || Residual::Unknown(podrunner_schema::report::Unknown::Unknown);
            r.disk_peak_bytes = Some(j.times.disk_peak.map_or_else(unknown, Residual::Count));
            if j.times.disk_peak.is_none() {
                r.unknown_reasons
                    .insert("disk_peak_bytes".into(), "no disk bound, so no disk check of the unit".into());
            }
            if self.egress.is_some() {
                r.net_rx_bytes = Some(Residual::Count(j.times.net_rx));
                r.net_tx_bytes = Some(Residual::Count(j.times.net_tx));
            } else {
                r.net_rx_bytes = Some(unknown());
                r.net_tx_bytes = Some(unknown());
                r.unknown_reasons.insert("net_rx_bytes".into(), "the host has no egress module".into());
                r.unknown_reasons.insert("net_tx_bytes".into(), "the host has no egress module".into());
            }
        }
        r.assigned = j.assigned.as_ref().zip(j.real.as_ref()).map(|(v, real)| Assigned {
            run_id: real.run_id,
            repository: real.repository.clone(),
            job_name: real.job_name.clone(),
            verdict: if *v == crate::assigned::Verdict::Keep { AssignedVerdict::Keep } else { AssignedVerdict::End },
            code: match v {
                crate::assigned::Verdict::End { code } => Some(code.clone()),
                crate::assigned::Verdict::Keep => None,
            },
        });
        let value = if self.sources[key.0].cfg.operator_controlled {
            serde_json::to_value(&r)
        } else {
            serde_json::to_value(r.public())
        };
        if let Ok(serde_json::Value::Object(m)) = value {
            self.send_report(key.0, &key.1, m);
        }
        crate::forget::keep(&mut self.reports, r);
    }
}
