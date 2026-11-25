//! The answer to the job-started hook of a unit whose runner got a job of
//! another run than its offer (T-493, githome's G-15). The forge, not the
//! source, picks the queued job that a runner takes. The source names the
//! trust facts of the real job; podrunner keeps the job only when those
//! facts give the tier of the unit, so a job of a fork never runs in a
//! trusted unit. The answer of a unit is given once and kept: a step of the
//! job that asks again gets the same answer.

use podrunner_schema::{ExitReason, UnitId};
use podrunner_source::{Assigned, RealJob, SourceCode};

use crate::audit;
use crate::engine::Slots;
use crate::state::State;

/// A question of the job-started hook of a unit: the unit, the JSON body
/// (`RealJob`), and where the answer goes (the JSON of [`Verdict::body`]).
pub type Question = (UnitId, Vec<u8>, std::sync::mpsc::SyncSender<String>);

/// The longest text of a fact of the real job.
const FACT_MAX: usize = 256;

/// The answer to the hook: run the job, or end it before its first step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Keep,
    End { code: String },
}

impl Verdict {
    fn end(code: &str) -> Verdict {
        Verdict::End { code: code.into() }
    }

    /// The JSON body of the answer to the hook.
    pub fn body(&self) -> String {
        match self {
            Verdict::Keep => r#"{"verdict":"keep"}"#.into(),
            Verdict::End { code } => serde_json::json!({ "verdict": "end", "code": code }).to_string(),
        }
    }
}

/// A code of a source: lower case, digits and `-`, at most 64 bytes.
fn clean_code(code: &str) -> String {
    let ok = !code.is_empty()
        && code.len() <= 64
        && code.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if ok { code.into() } else { "source-refused".into() }
}

fn plain(real: &RealJob) -> bool {
    let texts = [&real.job_name, &real.event, &real.repository, &real.actor];
    let head = real.head_repository.iter();
    texts.into_iter().chain(head).all(|s| s.len() <= FACT_MAX && !s.chars().any(char::is_control))
        && real.repository.split('/').count() == 2
}

impl Slots {
    /// Answer each question of the units (each tick).
    pub(crate) fn answer_questions(&mut self) {
        let questions = self.egress.as_ref().map(|e| e.questions()).unwrap_or_default();
        for (unit, body, reply) in questions {
            let verdict = match serde_json::from_slice::<RealJob>(&body) {
                Ok(real) => self.assigned(&unit, &real),
                Err(_) => Verdict::end("bad-request"),
            };
            // The hook waits a bounded time, then ends the job and drops its
            // end of the channel: the job then ends as the hook said.
            if let Err(std::sync::mpsc::TrySendError::Disconnected(_)) = reply.try_send(verdict.body()) {
                self.late(&unit);
            }
        }
    }

    /// The hook of `unit` gave up before the answer: it ended the job.
    fn late(&mut self, unit: &UnitId) {
        let now = self.now();
        if let Some(j) = self.jobs.values_mut().find(|j| j.unit.as_ref() == Some(unit)) {
            j.assigned = Some(Verdict::end("source-unavailable"));
            j.reason.get_or_insert(ExitReason::JobMismatch);
            j.term_ms.get_or_insert(now);
        }
    }

    /// The answer for the unit `unit`, whose runner got the job `real`.
    pub fn assigned(&mut self, unit: &UnitId, real: &RealJob) -> Verdict {
        let now = self.now();
        let key = self
            .jobs
            .iter()
            .find(|(_, j)| j.unit.as_ref() == Some(unit) && matches!(j.state, State::Started | State::Running))
            .map(|(k, _)| k.clone());
        let Some(key) = key else { return Verdict::end("unknown-unit") };
        if let Some(v) = &self.jobs[&key].assigned {
            return v.clone();
        }
        let verdict = self.decide(&key, real);
        let detail = format!(
            "run {} of {}: {}",
            real.run_id,
            real.repository.chars().take(FACT_MAX).collect::<String>(),
            match &verdict {
                Verdict::Keep => "keep".to_string(),
                Verdict::End { code } => format!("end {code}"),
            }
        );
        self.audit(audit::Kind::Assigned, key.0, &key.1, None, Some(detail));
        let offered = self.jobs[&key].offer.as_ref().is_some_and(|o| {
            let t = &o.trust_inputs;
            t.run_id == real.run_id && real.repository.eq_ignore_ascii_case(&format!("{}/{}", t.owner, t.repo))
        });
        if let Some(j) = self.jobs.get_mut(&key) {
            j.assigned = Some(verdict.clone());
            if !offered {
                j.real = Some(real.clone());
            }
            if verdict != Verdict::Keep {
                // The hook ends the job; the runner then ends the unit.
                j.reason.get_or_insert(ExitReason::JobMismatch);
                j.term_ms.get_or_insert(now);
            }
        }
        verdict
    }

    fn decide(&mut self, key: &(usize, podrunner_schema::JobId), real: &RealJob) -> Verdict {
        if !plain(real) {
            return Verdict::end("bad-request");
        }
        let j = &self.jobs[key];
        let (Some(offer), Some(p)) = (&j.offer, &j.placement) else { return Verdict::end("unknown-unit") };
        let t = &offer.trust_inputs;
        let same_repo = real.repository.eq_ignore_ascii_case(&format!("{}/{}", t.owner, t.repo));
        if same_repo && real.run_id == t.run_id {
            return Verdict::Keep;
        }
        let tier = p.tier;
        match self.sources[key.0].source.assigned(&key.1, real) {
            Err(e) if e.code == SourceCode::BadRequest => Verdict::end("not-the-offered-run"),
            Err(_) => Verdict::end("source-unavailable"),
            Ok(Assigned::End { code, .. }) => Verdict::End { code: clean_code(&code) },
            // The facts must be of the real job: a source that gives the
            // facts of the offer again must not pass a fork.
            Ok(Assigned::Keep(facts))
                if !real.repository.eq_ignore_ascii_case(&format!("{}/{}", facts.owner, facts.repo))
                    || facts.run_id != real.run_id =>
            {
                Verdict::end("facts-mismatch")
            }
            Ok(Assigned::Keep(facts)) => {
                let trust = self.cfg.trust_for(&self.sources[key.0].cfg, &facts);
                match trust.and_then(|t| podrunner_tier::intake(&facts, t, tier)) {
                    // The cache of the unit was made for the offer: the
                    // real job keeps it only on its own facts (T-480).
                    Ok(_) if !self.cache_holds(key, &facts) => Verdict::end("cache-mismatch"),
                    Ok(_) => Verdict::Keep,
                    Err(_) => Verdict::end("tier-mismatch"),
                }
            }
        }
    }
}
