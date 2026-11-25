//! The verbs of the command line that need more than one call: `stop
//! --wait`, `audit verify` and the status for a person.

use pod_core::failure::Class;
use serde_json::Value;

use crate::cli::{Opts, Out, err, ok, opts};
use crate::control::{self, Error, Request};
use crate::text::display;

/// `source restart --name NAME`: `serve` builds the source again from its
/// table, with its key file of now, and keeps its jobs (T-498).
pub(crate) fn source_restart(rest: &[String]) -> Result<Out, Out> {
    let o = opts("source restart", rest)?;
    let source = o.need("--name")?.display().to_string();
    let value = control::call(&o.socket()?, &Request::RestartSource { source }).map_err(|e| control_failure(&o, e))?;
    Ok(ok(format!("{value}\n")))
}

/// `stop --wait`: ask for the stop, then wait until no `serve` holds the
/// lock of the state directory (GitHub #54). A timeout is exit 75.
pub(crate) fn stop_wait(rest: &[String]) -> Result<Out, Out> {
    let o = opts("stop", rest)?;
    let state = o.need("--state")?;
    let timeout = match o.get("--timeout") {
        None => None,
        Some(t) => Some(std::time::Duration::from_secs(
            t.to_str()
                .and_then(|t| t.parse::<u64>().ok())
                .ok_or_else(|| err(Class::Usage, "--timeout needs a number of seconds"))?,
        )),
    };
    // A path that is not a directory has no serve and no lock (T-215).
    if !state.is_dir() {
        return Err(err(Class::Unavailable, format!("--state {}: not a directory", state.display())));
    }
    let started = std::time::Instant::now();
    let mut sent = false;
    loop {
        // No socket: serve is gone, or it holds the lock and has not opened
        // its socket yet (its start); ask again until it has one (T-629).
        if !sent {
            match control::call(&crate::serve::socket_of(&state), &Request::Stop) {
                Ok(_) => sent = true,
                Err(Error::Remote(f)) => return Err(refused(&f)),
                Err(_) => {}
            }
        }
        match crate::lock::is_free(&state) {
            Ok(true) if sent => return Ok(ok("stopped\n".into())),
            Ok(true) => return Ok(ok("no serve runs\n".into())),
            Ok(false) => {}
            Err(e) => return Err(err(Class::Unavailable, format!("the lock of the state directory: {}", e))),
        }
        if timeout.is_some_and(|t| started.elapsed() >= t) {
            return Err(match sent {
                true => err(Class::Timeout, "serve did not end in time: a job is not final"),
                false => err(
                    Class::Unavailable,
                    format!(
                        "serve has no control socket yet (it holds the lock of {} and starts): the stop did not \
                         reach it; run stop --wait again",
                        state.display()
                    ),
                ),
            });
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// `audit verify`: walk the chain of an audit file (GitHub #59). A break
/// is exit 76; the anchor is `SEQ:HASH` from the copy at the source.
pub(crate) fn audit_verify(rest: &[String]) -> Result<Out, Out> {
    let o = opts("audit verify", rest)?;
    let file = match (o.get("--file"), o.get("--state")) {
        (Some(f), _) => f,
        (None, Some(d)) => d.join("audit.jsonl"),
        (None, None) => {
            let usage = crate::manual::usage("audit verify");
            return Err(err(Class::Usage, format!("--state or --file is necessary\n{usage}")));
        }
    };
    let anchor = o.get("--anchor").map(|a| a.display().to_string());
    let anchor = match anchor.as_deref().map(|a| a.split_once(':')) {
        None => None,
        Some(Some((seq, hash))) if seq.parse::<u64>().is_ok() && hash.len() == 64 => {
            Some((seq.parse::<u64>().expect("a number"), hash.to_string()))
        }
        Some(_) => return Err(err(Class::Usage, "--anchor is SEQ:HASH, the hash in 64 hex digits")),
    };
    let text =
        std::fs::read(&file).map_err(|e| err(Class::NotFound, format!("the audit file {}: {e}", file.display())))?;
    match podrunner_slots::audit::verify(&text, anchor.as_ref().map(|(s, h)| (*s, h.as_str()))) {
        Ok(seq) => Ok(ok(format!("the chain holds: {seq} lines\n"))),
        Err(why) => Err(err(Class::Protocol, why)),
    }
}

/// The status for a person. Each value from a file or a forge goes through
/// `display`.
pub(crate) fn human(v: &Value) -> String {
    let s = |v: &Value| display(&v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()));
    let mut out = format!("host {}  draining {}  degraded {}\n", s(&v["host"]), v["draining"], s(&v["degraded"]));
    if v["no_tier_served"] == Value::Bool(true) {
        out.push_str("no-tier-served: no unit kind meets the floor of a source\n");
    }
    for src in v["sources"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "source {} tier {} arch {}  jobs {}  refused {}\n",
            s(&src["name"]),
            s(&src["tier"]),
            s(&src["arch"]),
            src["jobs"],
            src["refused"]
        ));
    }
    for k in v["kinds"].as_array().into_iter().flatten() {
        let p = &k["probe"];
        out.push_str(&format!(
            "kind {} slots {} busy {}  boundary {} principal {} hold {}\n",
            s(&k["name"]),
            k["slots"],
            k["busy"],
            s(&p["boundary"]),
            s(&p["principal"]),
            s(&p["egress_hold"])
        ));
        // The warm units of the kind (T-657).
        let w = &k["warm"];
        if w.is_object() {
            out.push_str(&format!("  warm target {} ready {} booting {}", w["target"], w["ready"], w["booting"]));
            if !w["paused_reason"].is_null() {
                out.push_str(&format!("  paused: {}", s(&w["paused_reason"])));
            }
            out.push('\n');
        }
    }
    for p in v["pins"].as_array().into_iter().flatten() {
        out.push_str(&format!("pin {} {} age {} s\n", s(&p["arch"]), s(&p["reference"]), p["age_s"]));
    }
    out
}

/// A control socket that does not answer: name the path, and a state
/// directory that does not exist (T-214).
/// The failure of a call of the control socket: an error that `serve`
/// answered is exit 77 (`refused`), with its code and its detail; no answer
/// is exit 69, with the path.
pub(crate) fn control_failure(o: &Opts, e: Error) -> Out {
    if let Error::Remote(f) = &e {
        return refused(f);
    }
    let socket = o.socket().map(|s| s.display().to_string()).unwrap_or_default();
    let why = match e {
        Error::Io(e) => e.to_string(),
        e => e.to_string(),
    };
    match o.get("--state") {
        Some(d) if !d.is_dir() => err(Class::Unavailable, format!("--state {}: not a directory", d.display())),
        _ => err(Class::Unavailable, format!("the control socket {socket}: {why}")),
    }
}

fn refused(f: &control::Fault) -> Out {
    err(Class::Refused, format!("{}: {}", f.code, display(&f.detail)))
}

#[cfg(test)]
mod tests {
    use crate::cli::run;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    /// T-214, T-215: each failure names its path; `stop --wait` on a path
    /// that is not a directory fails as `stop` does.
    #[test]
    fn each_failure_names_its_path() {
        let absent = "/nonexistent/podrunner-state";
        for verb in ["status", "drain", "stop"] {
            let o = run(&args(&[verb, "--state", absent]));
            assert_eq!(o.code, 69, "{verb}");
            assert!(o.stderr.contains(&format!("--state {absent}: not a directory")), "{verb}: {}", o.stderr);
        }
        let o = run(&args(&["stop", "--state", absent, "--wait"]));
        assert_eq!(o.code, 69, "stop --wait: {}{}", o.stdout, o.stderr);
        let empty = std::env::temp_dir().join(format!("podrunner-verbs-{}", std::process::id()));
        std::fs::create_dir_all(&empty).unwrap();
        let o = run(&args(&["status", "--state", empty.to_str().unwrap()]));
        assert!(o.stderr.contains(&format!("the control socket {}", empty.display())), "{}", o.stderr);
        let o = run(&args(&["check", "--config", "/nonexistent/podrunner.toml"]));
        assert!(o.stderr.contains("/nonexistent/podrunner.toml"), "{}", o.stderr);
        let o = run(&args(&["audit", "verify", "--file", "/nonexistent/audit.jsonl"]));
        assert!(o.stderr.contains("/nonexistent/audit.jsonl"), "{}", o.stderr);
        // T-216: a file of serve is not a file of the mint.
        let cfg = empty.join("podrunner.toml");
        std::fs::write(&cfg, "host = \"host-a\"\n").unwrap();
        let o = run(&args(&["mint-serve", "--config", cfg.to_str().unwrap()]));
        assert_eq!(o.code, 78);
        assert!(o.stderr.contains("is not a mint configuration"), "{}", o.stderr);
        let _ = std::fs::remove_dir_all(&empty);
    }
}
