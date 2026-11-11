//! The mint side of the remote mint (GitHub #31): it holds the source, and
//! a unit host reaches it with the source wire. Each host has its own
//! offers (the ledger), at most its slot count; the mint computes the tier
//! of each offer itself and withdraws one that the set may not run; it
//! gives one credential for each job id; each call is an audit line with
//! the host, the peer and the job id.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::time::Duration;

use podrunner_schema::{JobId, JobOffer, Tier};
use podrunner_source::wire::{self, Reply, Request};
use podrunner_source::{Event, JobSource, SourceCode, SourceError};
use podrunner_tier::TierConfig;
use serde_json::Value;

use crate::ledger::Ledger;

/// The mint side of one source.
pub struct Gate {
    source: Box<dyn JobSource>,
    set: Tier,
    trust: TierConfig,
    ledger: Ledger,
    /// The slot count of each host, by name.
    hosts: BTreeMap<String, u32>,
    /// The events of the offers of a host, kept for its next `next`.
    queued: BTreeMap<String, Vec<Event>>,
    audit: File,
}

fn err(code: SourceCode, detail: &str) -> SourceError {
    SourceError::new(code, detail)
}

impl Gate {
    pub fn new(
        source: Box<dyn JobSource>,
        set: Tier,
        trust: TierConfig,
        ledger: Ledger,
        hosts: BTreeMap<String, u32>,
        audit: File,
    ) -> Gate {
        Gate { source, set, trust, ledger, hosts, queued: BTreeMap::new(), audit }
    }

    /// Answer one request line of `host` (the name of its pinned key) from
    /// `peer` (its address). The reply line has its end of line.
    pub fn line(&mut self, host: &str, peer: &str, line: &[u8]) -> String {
        let reply = match wire::parse_request(line) {
            Err(e) => Reply::Error { code: e.code, detail: e.detail },
            Ok(req) => {
                let (op, job) = (req.op(), job_of(&req));
                self.write_audit(host, peer, op, job.as_ref());
                self.answer(host, req).unwrap_or_else(|e| Reply::Error { code: e.code, detail: e.detail })
            }
        };
        wire::reply_line(&reply)
    }

    fn write_audit(&mut self, host: &str, peer: &str, op: &str, job: Option<&JobId>) {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let line = serde_json::json!({ "t_s": t, "host": host, "peer": peer, "op": op, "job_id": job });
        // The ledger holds the state; a lost audit line stops no job.
        let _ = writeln!(self.audit, "{line}");
    }

    fn mine(&self, host: &str, job: &JobId) -> Result<(), SourceError> {
        if self.ledger.holder(job) != Some(host) {
            return Err(err(SourceCode::UnknownJob, "no offer of this job for this host"));
        }
        Ok(())
    }

    fn disk(e: std::io::Error) -> SourceError {
        SourceError::new(SourceCode::Unavailable, format!("the ledger: {}", e.kind()))
    }

    fn answer(&mut self, host: &str, req: Request) -> Result<Reply, SourceError> {
        let slots = *self.hosts.get(host).ok_or_else(|| err(SourceCode::SourceRevoked, "the host is not known"))?;
        if let Some(job) = job_of(&req) {
            self.mine(host, &job)?;
        }
        Ok(match req {
            Request::Hello { client, version } => {
                let h = self.source.hello(&client, &version)?;
                Reply::Hello { source: h.source, operator_controlled: h.operator_controlled }
            }
            Request::Next { capacity, wait_s } => {
                // Offers capped at the slot count of the host, whatever
                // `capacity` says.
                let free = (slots as usize).saturating_sub(self.ledger.outstanding(host)) as u32;
                let cap = capacity.min(free);
                let events = self.source.next(cap, Duration::from_secs(wait_s.into()))?;
                for e in events {
                    self.route(host, e)?;
                }
                Reply::Events { events: self.queued.remove(host).unwrap_or_default() }
            }
            Request::Ack { job_id } => self.source.ack(&job_id).map(|()| Reply::Done)?,
            Request::Claim { job_id, runner_name } => self.source.claim(&job_id, &runner_name).map(|()| Reply::Done)?,
            Request::Mint { job_id, runner_name } => {
                if self.ledger.minted(&job_id) {
                    return Err(err(SourceCode::CredentialAlreadyIssued, "one credential for each job id"));
                }
                let credential = self.source.mint(&job_id, &runner_name)?;
                // On disk before the answer: a crash after it cannot mint
                // the job again. A credential that the ledger cannot hold
                // is not given.
                self.ledger.mint(host, &job_id).map_err(Self::disk)?;
                Reply::Credential { credential }
            }
            Request::Report { job_id, report } => self.source.report(&job_id, &report).map(|()| Reply::Done)?,
            Request::Audit { line } => self.source.audit(&line).map(|()| Reply::Done)?,
            Request::Withdraw { job_id, reason } => {
                self.source.withdraw(&job_id, &reason)?;
                self.ledger.end(host, &job_id).map_err(Self::disk)?;
                Reply::Done
            }
            Request::Release { job_id } => {
                self.source.release(&job_id)?;
                self.ledger.end(host, &job_id).map_err(Self::disk)?;
                Reply::Done
            }
        })
    }

    /// An offer goes to `host` when its tier holds for the set; else the
    /// mint withdraws it. Another event goes to the host of its job.
    fn route(&mut self, host: &str, e: Event) -> Result<(), SourceError> {
        let to = match &e {
            Event::Offer { offer } => {
                let mut fields = offer.fields.clone();
                fields.insert("job_id".into(), Value::String(offer.job_id.as_str().into()));
                let tier = serde_json::from_value::<JobOffer>(Value::Object(fields))
                    .map_err(|_| podrunner_schema::RefusalCode::EventNotAllowed)
                    .and_then(|o| podrunner_tier::intake(&o.trust_inputs, &self.trust, self.set));
                if let Err(code) = tier {
                    let _ = self.source.withdraw(&offer.job_id, code.as_str());
                    return Ok(());
                }
                self.ledger.offer(host, &offer.job_id).map_err(Self::disk)?;
                host.to_string()
            }
            other => match self.ledger.holder(other.job_id()) {
                Some(h) => h.to_string(),
                None => return Ok(()),
            },
        };
        self.queued.entry(to).or_default().push(e);
        Ok(())
    }
}

fn job_of(r: &Request) -> Option<JobId> {
    match r {
        Request::Ack { job_id }
        | Request::Claim { job_id, .. }
        | Request::Mint { job_id, .. }
        | Request::Report { job_id, .. }
        | Request::Withdraw { job_id, .. }
        | Request::Release { job_id } => Some(job_id.clone()),
        Request::Hello { .. } | Request::Next { .. } | Request::Audit { .. } => None,
    }
}
