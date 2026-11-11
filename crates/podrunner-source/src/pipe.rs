//! The two ends of the wire: [`WireSource`] is a [`JobSource`] over a
//! stream; [`serve_line`] answers one request line from a [`JobSource`].

use std::io::{BufRead, Read, Write};
use std::time::Duration;

use podrunner_schema::{Credential, JobId};
use serde_json::{Map, Value};

use crate::wire::{self, LINE_MAX, Reply, Request};
use crate::{Event, Hello, JobSource, SourceCode, SourceError};

/// Answer one line of the wire. The reply line has its end of line.
pub fn serve_line(source: &mut dyn JobSource, line: &[u8]) -> String {
    let reply = match wire::parse_request(line) {
        Ok(req) => answer(source, req).unwrap_or_else(|e| Reply::Error { code: e.code, detail: e.detail }),
        Err(e) => Reply::Error { code: e.code, detail: e.detail },
    };
    wire::reply_line(&reply)
}

fn answer(s: &mut dyn JobSource, req: Request) -> Result<Reply, SourceError> {
    Ok(match req {
        Request::Hello { client, version } => {
            let h = s.hello(&client, &version)?;
            Reply::Hello { source: h.source, operator_controlled: h.operator_controlled }
        }
        Request::Next { capacity, wait_s } => {
            Reply::Events { events: s.next(capacity, Duration::from_secs(wait_s.into()))? }
        }
        Request::Ack { job_id } => s.ack(&job_id).map(|_| Reply::Done)?,
        Request::Claim { job_id, runner_name } => s.claim(&job_id, &runner_name).map(|_| Reply::Done)?,
        Request::Mint { job_id, runner_name } => Reply::Credential { credential: s.mint(&job_id, &runner_name)? },
        Request::Report { job_id, report } => s.report(&job_id, &report).map(|_| Reply::Done)?,
        Request::Audit { line } => s.audit(&line).map(|_| Reply::Done)?,
        Request::Withdraw { job_id, reason } => s.withdraw(&job_id, &reason).map(|_| Reply::Done)?,
        Request::Release { job_id } => s.release(&job_id).map(|_| Reply::Done)?,
    })
}

/// A source in another process: one request line out, one reply line in.
pub struct WireSource<R, W> {
    reader: R,
    writer: W,
}

impl<R: BufRead + Send, W: Write + Send> WireSource<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        WireSource { reader, writer }
    }

    fn call(&mut self, req: &Request) -> Result<Reply, SourceError> {
        let down = |e: std::io::Error| SourceError::new(SourceCode::Unavailable, format!("the wire: {}", e.kind()));
        self.writer.write_all(wire::request_line(req).as_bytes()).map_err(down)?;
        self.writer.flush().map_err(down)?;
        let mut line = Vec::new();
        // One byte over the limit tells a long line from a full one.
        let n = (&mut self.reader).take(LINE_MAX as u64 + 2).read_until(b'\n', &mut line).map_err(down)?;
        if n == 0 {
            return Err(SourceError::new(SourceCode::Unavailable, "the source closed the wire"));
        }
        match wire::parse_reply(&line)? {
            Reply::Error { code, detail } => Err(SourceError::new(code, detail)),
            other => Ok(other),
        }
    }

    fn done(&mut self, req: Request) -> Result<(), SourceError> {
        let op = req.op();
        match self.call(&req)? {
            Reply::Done => Ok(()),
            other => Err(unexpected(op, &other)),
        }
    }
}

fn unexpected(op: &str, r: &Reply) -> SourceError {
    let kind = match r {
        Reply::Hello { .. } => "hello",
        Reply::Events { .. } => "events",
        Reply::Credential { .. } => "credential",
        Reply::Done => "done",
        Reply::Error { .. } => "error",
    };
    SourceError::new(SourceCode::BadRequest, format!("the reply to {op} is {kind}"))
}

impl<R: BufRead + Send, W: Write + Send> JobSource for WireSource<R, W> {
    fn hello(&mut self, client: &str, version: &str) -> Result<Hello, SourceError> {
        match self.call(&Request::Hello { client: client.into(), version: version.into() })? {
            Reply::Hello { source, operator_controlled } => Ok(Hello { source, operator_controlled }),
            other => Err(unexpected("hello", &other)),
        }
    }

    fn next(&mut self, capacity: u32, wait: Duration) -> Result<Vec<Event>, SourceError> {
        let wait_s = u32::try_from(wait.as_secs()).unwrap_or(u32::MAX);
        match self.call(&Request::Next { capacity, wait_s })? {
            Reply::Events { events } => Ok(events),
            other => Err(unexpected("next", &other)),
        }
    }

    fn ack(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.done(Request::Ack { job_id: job.clone() })
    }

    fn claim(&mut self, job: &JobId, runner_name: &str) -> Result<(), SourceError> {
        self.done(Request::Claim { job_id: job.clone(), runner_name: runner_name.into() })
    }

    fn mint(&mut self, job: &JobId, runner_name: &str) -> Result<Credential, SourceError> {
        match self.call(&Request::Mint { job_id: job.clone(), runner_name: runner_name.into() })? {
            Reply::Credential { credential } => Ok(credential),
            other => Err(unexpected("mint", &other)),
        }
    }

    fn report(&mut self, job: &JobId, report: &Map<String, Value>) -> Result<(), SourceError> {
        self.done(Request::Report { job_id: job.clone(), report: report.clone() })
    }

    fn audit(&mut self, line: &Map<String, Value>) -> Result<(), SourceError> {
        self.done(Request::Audit { line: line.clone() })
    }

    fn withdraw(&mut self, job: &JobId, reason: &str) -> Result<(), SourceError> {
        self.done(Request::Withdraw { job_id: job.clone(), reason: reason.into() })
    }

    fn release(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.done(Request::Release { job_id: job.clone() })
    }
}
