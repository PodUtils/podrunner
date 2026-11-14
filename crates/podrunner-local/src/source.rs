//! The local source as a job source of `podrunner-source`: the offers of
//! the ready cells, and the states of a job from its offer to its release.

use std::time::Duration;

use podrunner_schema::{Credential, JobId};
use podrunner_source::{Event, Hello, JobSource, SourceCode, SourceError};
use serde_json::{Map, Value};

use crate::{LocalSource, State};

impl JobSource for LocalSource {
    fn hello(&mut self, _: &str, _: &str) -> Result<Hello, SourceError> {
        Ok(Hello { source: self.name.clone(), operator_controlled: true })
    }

    fn next(&mut self, capacity: u32, _wait: Duration) -> Result<Vec<Event>, SourceError> {
        let mut events = Vec::new();
        for _ in 0..capacity {
            let Some(offer) = self.pending.pop_front() else { break };
            self.states.insert(offer.job_id.clone(), State::Offered);
            events.push(Event::Offer { offer });
        }
        Ok(events)
    }

    fn ack(&mut self, job: &JobId) -> Result<(), SourceError> {
        if self.state(job)? == State::Offered {
            self.states.insert(job.clone(), State::Acked);
        }
        Ok(())
    }

    fn claim(&mut self, job: &JobId, _runner_name: &str) -> Result<(), SourceError> {
        if self.state(job)? == State::Claimed {
            return Err(SourceError::new(SourceCode::AlreadyClaimed, format!("{job} is claimed")));
        }
        self.states.insert(job.clone(), State::Claimed);
        Ok(())
    }

    fn mint(&mut self, job: &JobId, runner_name: &str) -> Result<Credential, SourceError> {
        if self.state(job)? != State::Claimed {
            return Err(SourceError::new(SourceCode::NotClaimed, format!("{job} is not claimed")));
        }
        Ok((self.minter)(job, runner_name))
    }

    fn report(&mut self, job: &JobId, report: &Map<String, Value>) -> Result<(), SourceError> {
        self.reports.push((job.clone(), report.clone()));
        Ok(())
    }

    fn audit(&mut self, line: &Map<String, Value>) -> Result<(), SourceError> {
        self.audits.push(line.clone());
        Ok(())
    }

    fn withdraw(&mut self, job: &JobId, _reason: &str) -> Result<(), SourceError> {
        if self.state(job)? == State::Claimed {
            return Err(SourceError::new(SourceCode::AlreadyClaimed, format!("{job} is claimed: release it")));
        }
        self.states.remove(job);
        Ok(())
    }

    fn release(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.state(job)?;
        self.states.remove(job);
        Ok(())
    }
}
