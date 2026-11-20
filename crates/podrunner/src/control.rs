//! The control socket (GitHub #22): `pod-control` protocol 1 of podcrates
//! on a unix socket of mode 0600 (T-576). A peer with another uid is
//! refused. `version`, `methods` and `auth` are local to the server;
//! `status`, `metrics`, `drain` and `stop` go to the supervisor, and the
//! methods of podrunner are the [`Extra`] rows of [`catalogue`].
//!
//! A peer of the uid of podrunner has the master tier: it can call each
//! method, `restart-source` too (a key file read again is a new
//! credential, so that row has the master tier).
//!
//! Where units run as the uid of podrunner (a shared principal), the peer
//! check protects nothing against a unit: `docs/security/` says so.

use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value, json};

pub use pod_control::{
    Catalogue, Client, ClientSettings, Code, Context, Drain, Error, Extra, Fault, Host, Incoming, Metrics, Outcome,
    Settings, Snapshot, Tier,
};

/// The name of the program in `version`.
pub const PROGRAM: &str = "podrunner";

/// A request of the command line or of `serve` itself, with the method of
/// the wire that carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Status,
    /// `drain {"exit": false}`: no new claim; `serve` keeps running.
    Drain,
    /// `drain {"exit": true}`: no new claim, each running job ends, then
    /// `serve` exits. The method `stop` does the same.
    Stop,
    Capacity,
    /// The operator looked at the host: start jobs again.
    ClearDegraded,
    Withdraw {
        job_id: String,
    },
    /// Build the source `source` again from the configuration, with its
    /// key file of now, and keep its jobs (T-498).
    RestartSource {
        source: String,
    },
    /// The source `source` takes no offer; its jobs go on (T-501).
    DrainSource {
        source: String,
    },
    /// The source `source` takes offers again; a stopped source is built
    /// again first.
    ResumeSource {
        source: String,
    },
    /// Drain the source `source`, and end its program when its last job
    /// ends (T-501).
    StopSource {
        source: String,
    },
}

/// The extra methods of podrunner: each name, its tier and its summary.
const EXTRAS: &[(&str, Tier, &str)] = &[
    ("capacity", Tier::Read, "The free slots of all unit kinds."),
    ("clear-degraded", Tier::Operate, "The operator looked at the host: start jobs again."),
    ("withdraw", Tier::Operate, "{\"job_id\"}: withdraw one job (with the scale-set source)."),
    ("drain-source", Tier::Operate, "{\"source\"}: the source takes no offer; its jobs go on."),
    ("resume-source", Tier::Operate, "{\"source\"}: the source takes offers again."),
    ("stop-source", Tier::Operate, "{\"source\"}: drain the source and end its program after its last job."),
    ("restart-source", Tier::Master, "{\"source\"}: build the source again with its key file of now."),
];

/// The catalogue of podrunner: the rows of `pod-control` and `EXTRAS`.
pub fn catalogue() -> Catalogue {
    let extras = EXTRAS
        .iter()
        .map(|(name, tier, summary)| {
            let mut e = Extra::new(name, *tier, summary);
            // A key file read again is a new credential: it outlives the
            // token of the caller.
            e.outlives = *tier == Tier::Master;
            e
        })
        .collect();
    Catalogue::new(extras).expect("the extras of podrunner are a valid catalogue")
}

/// The names of the extra methods, for the conformance check.
pub fn extra_names() -> Vec<String> {
    EXTRAS.iter().map(|(n, ..)| n.to_string()).collect()
}

/// The settings of the server of `serve`: 8 clients, a line of 64 KiB,
/// 5 s for a line and 5 s for the supervisor, mode 0600.
pub fn settings() -> Settings {
    let version = format!("{} ({})", env!("PODRUNNER_VERSION"), env!("PODRUNNER_COMMIT"));
    let mut s = Settings::new(PROGRAM, version);
    s.same_user = Tier::Master;
    s
}

fn bad_params() -> Fault {
    Fault::new(Code::BadParams, "the params do not fit the method")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceParams {
    source: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WithdrawParams {
    job_id: String,
}

fn no_params(params: &Value) -> Result<(), Fault> {
    match params.as_object() {
        Some(m) if m.is_empty() => Ok(()),
        _ => Err(bad_params()),
    }
}

impl Request {
    /// The request of the extra method `name`. The detail of an error holds
    /// no byte of the params.
    pub fn of_extra(name: &str, params: &Value) -> Result<Request, Fault> {
        let source =
            || serde_json::from_value::<SourceParams>(params.clone()).map(|p| p.source).map_err(|_| bad_params());
        Ok(match name {
            "capacity" => no_params(params).map(|()| Request::Capacity)?,
            "clear-degraded" => no_params(params).map(|()| Request::ClearDegraded)?,
            "withdraw" => {
                let p: WithdrawParams = serde_json::from_value(params.clone()).map_err(|_| bad_params())?;
                Request::Withdraw { job_id: p.job_id }
            }
            "drain-source" => Request::DrainSource { source: source()? },
            "resume-source" => Request::ResumeSource { source: source()? },
            "stop-source" => Request::StopSource { source: source()? },
            "restart-source" => Request::RestartSource { source: source()? },
            _ => return Err(Fault::unknown_method()),
        })
    }

    /// The method of the wire and its params.
    pub fn method(&self) -> (&'static str, Value) {
        let source = |s: &str| json!({ "source": s });
        match self {
            Request::Status => ("status", json!({})),
            Request::Drain => ("drain", json!({ "exit": false })),
            Request::Stop => ("drain", json!({ "exit": true })),
            Request::Capacity => ("capacity", json!({})),
            Request::ClearDegraded => ("clear-degraded", json!({})),
            Request::Withdraw { job_id } => ("withdraw", json!({ "job_id": job_id })),
            Request::RestartSource { source: s } => ("restart-source", source(s)),
            Request::DrainSource { source: s } => ("drain-source", source(s)),
            Request::ResumeSource { source: s } => ("resume-source", source(s)),
            Request::StopSource { source: s } => ("stop-source", source(s)),
        }
    }
}

/// Read one request line of a client, as the server reads it: the frame
/// of `pod-control`, then the method of podrunner and its params.
pub fn parse(line: &[u8]) -> Result<Request, Fault> {
    let text = std::str::from_utf8(line).map_err(|_| Fault::new(Code::NotUtf8, "a line is UTF-8"))?;
    let text = text.strip_suffix('\n').unwrap_or(text);
    if text.len() > pod_control::LINE_MAX {
        return Err(Fault::new(Code::TooLong, "a line over the cap"));
    }
    match pod_control::wire::parse(text).map_err(|(_, f)| f)? {
        pod_control::wire::Inbound::Answer { .. } => Err(Fault::new(Code::BadRequest, "an answer with no call back")),
        pod_control::wire::Inbound::Request(r) => match r.method.as_str() {
            "status" => no_params(&r.params).map(|()| Request::Status),
            "stop" => no_params(&r.params).map(|()| Request::Stop),
            "drain" => {
                let d: Drain = serde_json::from_value(r.params).map_err(|_| bad_params())?;
                Ok(if d.exit { Request::Stop } else { Request::Drain })
            }
            name => Request::of_extra(name, &r.params),
        },
    }
}

/// What answers a request: the supervisor.
pub trait Handler {
    fn handle(&mut self, req: &Request) -> Outcome;
}

/// The [`Host`] of `pod-control` over a [`Handler`].
pub struct AsHost<'a, H: Handler + ?Sized>(pub &'a mut H);

impl<H: Handler + ?Sized> Host for AsHost<'_, H> {
    fn status(&mut self, _: &mut Context) -> Outcome {
        self.0.handle(&Request::Status)
    }

    fn metrics(&mut self, _: &mut Context) -> Result<Metrics, Fault> {
        self.0.handle(&Request::Status).map(|s| metrics_of(&s))
    }

    fn reload(&mut self, _: &mut Context) -> Outcome {
        Err(Fault::new(
            Code::Unsupported,
            "serve reads its configuration at the start; restart-source builds one source again",
        ))
    }

    fn drain(&mut self, drain: Drain, _: &mut Context) -> Outcome {
        self.0.handle(if drain.exit { &Request::Stop } else { &Request::Drain })
    }

    fn stop(&mut self, _: &mut Context) -> Outcome {
        self.0.handle(&Request::Stop)
    }

    fn extra(&mut self, name: &str, params: &Value, _: &mut Context) -> Outcome {
        self.0.handle(&Request::of_extra(name, params)?)
    }
}

/// The metrics of a `status.v1` value: gauges of now, and the counters
/// that only grow.
pub fn metrics_of(status: &Value) -> Metrics {
    let mut m = Metrics::default();
    let flag = |k: &str| i64::from(status[k].as_bool().unwrap_or(false));
    let num = |v: &Value| v.as_u64().map_or(0, |n| i64::try_from(n).unwrap_or(i64::MAX));
    m.gauges.insert("draining".into(), flag("draining"));
    m.gauges.insert("degraded".into(), i64::from(!status["degraded"].is_null()));
    m.gauges.insert("no_tier_served".into(), flag("no_tier_served"));
    m.gauges.insert("units".into(), num(&status["units"]));
    m.gauges.insert("spooled_reports".into(), num(&status["spooled_reports"]));
    let kinds = status["kinds"].as_array().map(Vec::as_slice).unwrap_or_default();
    m.gauges.insert("slots".into(), kinds.iter().map(|k| num(&k["slots"])).sum());
    m.gauges.insert("busy".into(), kinds.iter().map(|k| num(&k["busy"])).sum());
    let empty = Map::new();
    for s in status["sources"].as_array().map(Vec::as_slice).unwrap_or_default() {
        for (state, n) in s["jobs"].as_object().unwrap_or(&empty) {
            *m.gauges.entry(format!("jobs_{state}")).or_default() += num(n);
        }
        for (why, n) in s["refused"].as_object().unwrap_or(&empty) {
            *m.counters.entry(format!("refused_{why}")).or_default() += n.as_u64().unwrap_or(0);
        }
    }
    m.counters.insert("ended_without_completion".into(), status["ended_without_completion"].as_u64().unwrap_or(0));
    m
}

/// Serve the control at `path`. A stale socket goes first; a live socket
/// is `AddrInUse`, and a path that is not a socket is `AlreadyExists`:
/// neither is removed. The drop of the server removes its socket.
pub fn serve(path: &Path) -> std::io::Result<(pod_control::unix::Server, Incoming)> {
    pod_control::unix::serve(path, settings(), catalogue())
}

/// Send one request to the socket at `path`, and give its result.
pub fn call(path: &Path, req: &Request) -> Result<Value, Error> {
    let (method, params) = req.method();
    Client::connect_unix(path, ClientSettings::default())?.call(method, params)
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
