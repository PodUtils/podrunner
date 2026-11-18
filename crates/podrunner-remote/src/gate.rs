//! The mint side of the remote mint (GitHub #31): it holds the source, and
//! a unit host reaches it with the source wire. Each host has its own
//! offers (the ledger), at most its slot count; the mint computes the tier
//! of each offer itself and withdraws one that the set may not run; it
//! gives one credential for each job id; each call is an audit line with
//! the host, the peer and the job id.

use std::collections::{BTreeMap, BTreeSet};
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
    /// The time in seconds of the Unix epoch.
    now: Box<dyn Fn() -> u64 + Send>,
    reconcile: Reconcile,
    /// The credentials with an alert, once each.
    alerted: BTreeSet<JobId>,
    /// The late credentials of each host, also one that started or was
    /// released after its alert (#245).
    late_of: std::collections::BTreeMap<String, usize>,
    /// The hosts that the reconciliation revoked: each call is
    /// `source-revoked` until the operator starts the mint again.
    revoked: BTreeSet<String>,
}

/// A credential with no start after `after_s` is an alert; `revoke_after`
/// alerts of one host revoke it (GitHub #31).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reconcile {
    pub after_s: u64,
    pub revoke_after: usize,
}

impl Default for Reconcile {
    fn default() -> Self {
        // `start_s` of slots (120 s) and a margin.
        Reconcile { after_s: 300, revoke_after: 3 }
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
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
        Gate {
            source,
            set,
            trust,
            ledger,
            hosts,
            queued: BTreeMap::new(),
            audit,
            now: Box::new(unix_now),
            reconcile: Reconcile::default(),
            alerted: BTreeSet::new(),
            late_of: std::collections::BTreeMap::new(),
            revoked: BTreeSet::new(),
        }
    }

    /// Another clock and other limits of the reconciliation.
    pub fn with_reconcile(mut self, now: Box<dyn Fn() -> u64 + Send>, r: Reconcile) -> Gate {
        (self.now, self.reconcile) = (now, r);
        self
    }

    /// An alert for each credential with no start in time; a host with
    /// `revoke_after` alerts is revoked.
    fn check_starts(&mut self) {
        let now = (self.now)();
        let late: Vec<(JobId, String)> = self
            .ledger
            .pending()
            .filter(|(j, _, t)| now.saturating_sub(*t) >= self.reconcile.after_s && !self.alerted.contains(*j))
            .map(|(j, h, _)| (j.clone(), h.to_string()))
            .collect();
        for (job, host) in late {
            self.late(&job, &host, now, "credential-with-no-start");
        }
    }

    /// A credential of `job` that ends its wait now (it starts, or goes
    /// back): one past the limit is late, as one that never starts (#245).
    fn settle(&mut self, job: &JobId) {
        let now = (self.now)();
        let late = self.ledger.pending().find(|(j, _, _)| *j == job).and_then(|(_, h, t)| {
            (now.saturating_sub(t) >= self.reconcile.after_s && !self.alerted.contains(job)).then(|| h.to_string())
        });
        if let Some(host) = late {
            self.late(job, &host, now, "credential-used-late");
        }
    }

    /// One late credential of `host`: an alert; the late credentials of a
    /// host count, also one that starts or goes back after its alert, and
    /// `revoke_after` of them revoke the host.
    fn late(&mut self, job: &JobId, host: &str, now: u64, alert: &str) {
        self.alerted.insert(job.clone());
        let line = serde_json::json!({ "t_s": now, "alert": alert, "host": host, "job_id": job });
        let _ = writeln!(self.audit, "{line}");
        let count = {
            let n = self.late_of.entry(host.to_string()).or_default();
            *n += 1;
            *n
        };
        if count >= self.reconcile.revoke_after && self.revoked.insert(host.to_string()) {
            let line = serde_json::json!({ "t_s": now, "alert": "host-revoked", "host": host });
            let _ = writeln!(self.audit, "{line}");
            // Each offer of the host goes back; the source removes the
            // runner of each one that it minted (the sidecar does).
            for job in self.ledger.offers_of(host) {
                let _ = self.source.release(&job);
                let _ = self.ledger.end(host, &job);
            }
        }
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
        let t = (self.now)();
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
        self.check_starts();
        if self.revoked.contains(host) {
            return Err(err(SourceCode::SourceRevoked, "the host is revoked: credentials with no start"));
        }
        let slots = *self.hosts.get(host).ok_or_else(|| err(SourceCode::SourceRevoked, "the host is not known"))?;
        if let Some(job) = job_of(&req) {
            self.mine(host, &job)?;
        }
        Ok(match req {
            Request::Hello { client, version, host } => {
                let h = match &host {
                    Some(facts) => self.source.hello_host(&client, &version, facts)?,
                    None => self.source.hello(&client, &version)?,
                };
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
                self.ledger.mint(host, &job_id, (self.now)()).map_err(Self::disk)?;
                Reply::Credential { credential }
            }
            Request::Report { job_id, report } => self.source.report(&job_id, &report).map(|()| Reply::Done)?,
            Request::Audit { line } => self.source.audit(&line).map(|()| Reply::Done)?,
            Request::Withdraw { job_id, reason } => {
                self.settle(&job_id);
                self.source.withdraw(&job_id, &reason)?;
                self.ledger.end(host, &job_id).map_err(Self::disk)?;
                Reply::Done
            }
            Request::Release { job_id } => {
                self.settle(&job_id);
                self.source.release(&job_id)?;
                self.ledger.end(host, &job_id).map_err(Self::disk)?;
                Reply::Done
            }
            // The job of a host only (`mine` above): the source answers.
            Request::Assigned { job_id, real } => match self.source.assigned(&job_id, &real)? {
                podrunner_source::Assigned::Keep(trust_inputs) => Reply::Keep { trust_inputs },
                podrunner_source::Assigned::End { code, detail } => Reply::End { code, detail },
            },
        })
    }

    /// An offer goes to `host` when its tier holds for the set; else the
    /// mint withdraws it. Another event goes to the host of its job.
    fn route(&mut self, host: &str, e: Event) -> Result<(), SourceError> {
        let to = match &e {
            Event::Offer { offer } => {
                let mut fields = offer.fields.clone();
                fields.insert("job_id".into(), Value::String(offer.job_id.as_str().into()));
                // A bad offer is `bad-request` with the text of the parser,
                // not the code of a refused event (T-188).
                let refused = match serde_json::from_value::<JobOffer>(Value::Object(fields)) {
                    Err(e) => Some(("bad-request", e.to_string())),
                    Ok(o) => podrunner_tier::intake(&o.trust_inputs, &self.trust, self.set)
                        .err()
                        .map(|c| (c.as_str(), String::new())),
                };
                if let Some((code, detail)) = refused {
                    // Each withdrawal leaves a trace (T-170).
                    let line = serde_json::json!({
                        "t_s": (self.now)(), "host": host, "op": "withdraw", "job_id": offer.job_id,
                        "code": code, "detail": detail,
                    });
                    let _ = writeln!(self.audit, "{line}");
                    eprintln!("podrunner: mint: withdraw {} for {host}: {code} {detail}", offer.job_id);
                    let _ = self.source.withdraw(&offer.job_id, code);
                    return Ok(());
                }
                self.ledger.offer(host, &offer.job_id).map_err(Self::disk)?;
                host.to_string()
            }
            other => match self.ledger.holder(other.job_id()).map(str::to_string) {
                Some(h) => {
                    if matches!(other, Event::Started { .. }) {
                        self.settle(other.job_id());
                        self.ledger.start(&h, other.job_id()).map_err(Self::disk)?;
                    }
                    h
                }
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
        | Request::Release { job_id }
        | Request::Assigned { job_id, .. } => Some(job_id.clone()),
        Request::Hello { .. } | Request::Next { .. } | Request::Audit { .. } => None,
    }
}
