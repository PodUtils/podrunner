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
    /// The hashes of the long words of the credential, for the scrub of
    /// `log_tail`; no credential stays after the start.
    pub marks: crate::log_tail::Marks,
    /// The scrubbed end of the log of a unit that failed before its first
    /// step.
    pub log_tail: Option<String>,
    /// The cache of the job (T-480).
    pub cache: Option<crate::cache::Grant>,
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
        r.claim_to_first_step_s = secs(j.claimed_ms, j.first_step_ms);
        r.job_images = j.images.clone();
        // The use of the unit, when a unit ran (T-108).
        if unit_ran {
            let [(cpu, cpu_why), (peak, peak_why)] = j.usage.report();
            r.cpu_s = Some(cpu);
            r.peak_rss_mib = Some(peak);
            for (field, why) in [("cpu_s", cpu_why), ("peak_rss_mib", peak_why)] {
                if let Some(why) = why {
                    r.unknown_reasons.insert(field.into(), why);
                }
            }
        }
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
        // Each absent time of `TIMES` gets the reason of its first absent
        // end (T-595): githome reads why a time is not there (G-19).
        let gone = |ms: Option<u64>, why: &'static str| ms.is_none().then_some(why);
        let claim = gone(j.claimed_ms, "the job was not claimed");
        let start = gone(j.started_ms, "the unit did not start");
        // A `program` source with no hook sends no `started` event, also
        // for a job that ran its steps (T-568); a payload with no
        // job-started hook of the runner image sends no mark (T-660).
        let step = gone(
            j.first_step_ms,
            "no first step: no started event of the source, no container hook and no mark of the job-started hook came",
        );
        let run_start = gone(j.first_step_ms.or(j.started_ms), "the unit did not start");
        let end = gone(j.times.ended_ms, "the unit did not end before the report");
        let destroy = gone(j.times.destroyed_ms, "the unit was not destroyed before the report");
        for (field, time, why) in [
            ("claim_to_first_step_s", r.claim_to_first_step_s, claim.or(step)),
            ("prepare_s", r.prepare_s, claim.or(start)),
            ("start_s", r.start_s, start.or(step)),
            ("run_s", r.run_s, run_start.or(end)),
            ("destroy_s", r.destroy_s, end.or(destroy)),
        ] {
            if let (None, Some(why)) = (time, why) {
                r.unknown_reasons.insert(field.into(), why.into());
            }
        }
        r.log_tail = j.times.log_tail.clone();
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
            crate::cache::report(j.times.cache.as_ref(), &mut r);
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

    /// Read the log of the unit of `key` before its destroy, once, when its
    /// exit code does not tell how the job ended. The runner exits 0 after
    /// a job that the job-started hook ended (T-552), with no job (T-567),
    /// and after a job that failed, was cancelled or was abandoned (T-568,
    /// T-569). A unit that ended with no step and not well gets `log_tail`
    /// (T-497). The read has a bound of bytes and of time (T-570).
    pub(crate) fn read_unit_log(&mut self, key: &Key, unit: &podrunner_schema::UnitId, kind: usize) {
        let j = &self.jobs[key];
        // A cancel is no failure. A sandbox has no runner.
        let early = j.started_ms.is_some() && j.first_step_ms.is_none() && j.reason != Some(ExitReason::Cancelled);
        let zero = j.reason == Some(ExitReason::Success) && j.offer.as_ref().is_none_or(|o| o.sandbox.is_none());
        if !(early || zero) {
            return;
        }
        let mut scan = crate::log_tail::Scan::default();
        let t0 = std::time::Instant::now();
        let read = self.kinds[kind]
            .driver
            .unit_log(unit)
            .and_then(|log| log.read_bounded(log_read_max(), LOG_READ_TIME, &mut |b| scan.feed(b)));
        let read = match read {
            Ok(read) => read,
            // A kind that keeps no log has nothing to tell.
            Err(e) if e.code == podrunner_unit::UnitCode::Unsupported => return,
            Err(e) => {
                // The exit code tells; a line names why the log does not
                // (the incus tier of a podvm before `0.2.0-beta.38` refuses a
                // log over 64 MiB).
                let why = format!("the log of the unit tells no result of the job: {e}");
                eprintln!("podrunner: {unit}: {why}");
                if zero {
                    self.audit(crate::audit::Kind::JobCompleted, key.0, &key.1, None, Some(why));
                }
                return;
            }
        };
        scan.end();
        let cut = match read.cut {
            None => String::new(),
            Some(podrunner_unit::Cut::Bytes) => format!(", cut at the bound of {} bytes", log_read_max()),
            Some(podrunner_unit::Cut::Time) => format!(", cut at the bound of {} s", LOG_READ_TIME.as_secs()),
        };
        eprintln!(
            "podrunner: {unit}: read {} bytes of the unit log in {} ms{cut}",
            read.bytes,
            t0.elapsed().as_millis()
        );
        let ended = if early { scan.hook.clone() } else { None };
        if let Some(code) = &ended {
            let why = format!("the job-started hook ended the job: {code}");
            self.audit(crate::audit::Kind::JobMismatch, key.0, &key.1, None, Some(why));
        }
        // In a cut log, a line of a job can be after the cut.
        let idle = early && zero && ended.is_none() && read.cut.is_none() && scan.took_no_job();
        if idle {
            let why = "the runner exited 0 with no job: no `Running job:` line in its log";
            eprintln!("podrunner: {unit}: {why}");
            self.audit(crate::audit::Kind::Refuse, key.0, &key.1, None, Some(why.into()));
        }
        let told = if zero && ended.is_none() && !idle { told(&scan, read.cut.is_some()) } else { None };
        if let Some((_, why)) = &told {
            eprintln!("podrunner: {unit}: {why}");
            self.audit(crate::audit::Kind::JobCompleted, key.0, &key.1, None, Some(why.clone()));
        } else if zero && ended.is_none() && read.cut.is_some() {
            let why = format!("the log of the unit tells no failure of the job before its cut{cut}");
            eprintln!("podrunner: {unit}: {why}");
            self.audit(crate::audit::Kind::JobCompleted, key.0, &key.1, None, Some(why));
        }
        let Some(job) = self.jobs.get_mut(key) else { return };
        if ended.is_some() && matches!(job.reason, None | Some(ExitReason::Success | ExitReason::Failure)) {
            job.reason = Some(ExitReason::JobMismatch);
        }
        if idle {
            job.reason = Some(ExitReason::UnitFailed);
        }
        if let Some((reason, _)) = told {
            job.reason = Some(reason);
        }
        // A job that ran a step has its log at the forge: no `log_tail`
        // (the G-19 contract: a unit that failed before its first step).
        // With no first-step event (a `program` source with no container
        // hook) each job counts as early, so each one that failed gets a
        // `log_tail`: such a source has no other log of the job (T-568).
        if early && !matches!(job.reason, Some(ExitReason::Success | ExitReason::Cancelled)) {
            job.times.log_tail = Some(scan.tail(&job.times.marks));
        }
    }
}

/// The most bytes of the log of a unit that slots reads for one job
/// (T-570). The lines that tell the end of a job are short, and the
/// listener prints `Running job:` before a step runs.
const LOG_READ_MAX: u64 = 64 << 20;

/// The most time of the read of the log of a unit: the tick waits for it.
const LOG_READ_TIME: std::time::Duration = std::time::Duration::from_secs(10);

#[cfg(test)]
thread_local! {
    /// A smaller bound of bytes for a test of the cut.
    pub(crate) static LOG_READ_BOUND: std::cell::Cell<u64> = const { std::cell::Cell::new(LOG_READ_MAX) };
}

fn log_read_max() -> u64 {
    #[cfg(test)]
    return LOG_READ_BOUND.get();
    #[cfg(not(test))]
    LOG_READ_MAX
}

/// The exit reason that the log of a runner that exited 0 gives, with why.
/// A cut log tells only a result that gives a reason.
fn told(scan: &crate::log_tail::Scan, cut: bool) -> Option<(ExitReason, String)> {
    let said = "the runner exited 0 and its log says that the job completed with result";
    match scan.result() {
        Some(r) => r.reason().map(|reason| (reason, format!("{said}: {r:?}"))),
        // The listener prints a completed line on each path of a job that
        // ran on the worker; it prints none when it gave up before the
        // worker ran it (the first renewal of the lock failed: an
        // abandoned job, T-569).
        None if scan.running() && !cut => Some((
            ExitReason::Failure,
            "the runner exited 0 after `Running job:` and its log has no completed line of the job".into(),
        )),
        None => None,
    }
}
