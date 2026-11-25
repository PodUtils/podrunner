//! Warm units (T-487). A warm unit is a unit that `start` would make, with
//! nothing of a job: no credential, no job environment, no job argv. The
//! claim of one job gives it these; then it runs that job and is
//! destroyed. The types keep the credential out of `warm`: [`WarmSpec`]
//! has no field for it.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use podrunner_schema::{Credential, UnitId};

use crate::limits::Limits;
use crate::{Prepared, StartSpec, UnitCode, UnitDriver, UnitError};

/// What a warm unit gets before its job.
#[derive(Debug, Clone)]
pub struct WarmSpec {
    pub unit: UnitId,
    pub image: Prepared,
    /// The socket directory of the unit: its egress socket leads nowhere
    /// until the claim.
    pub sockets: Option<PathBuf>,
    /// The variables of the unit only (`PODRUNNER_UNIT`). Never a
    /// credential, never a variable of a job.
    pub env: BTreeMap<String, String>,
}

/// What the claim of a warm unit gives it: the rest of a [`StartSpec`].
/// It has no mounts: a job with a mount never takes a warm unit.
#[derive(Debug, Clone)]
pub struct ClaimSpec {
    pub argv: Vec<String>,
    /// The environment of the job, as `StartSpec::env`.
    pub env: BTreeMap<String, String>,
    /// The files for the runner root, mode 0600: only here.
    pub credential: Credential,
    /// The limits of the offer; a warm unit fits it only when
    /// `applied_limits` of the offer equals the limits of the unit.
    pub limits: Limits,
}

/// The variables of a unit that a warm unit gets before its claim.
pub const UNIT_VARS: [&str; 1] = ["PODRUNNER_UNIT"];

/// The two halves of a start: the warm part and the claim. A start with a
/// mount has no warm form (`unsupported`).
pub fn split(spec: &StartSpec) -> Result<(WarmSpec, ClaimSpec), UnitError> {
    if !spec.mounts.is_empty() {
        return Err(UnitError::new(UnitCode::Unsupported, "a warm unit takes no mount"));
    }
    let env = spec.env.iter().filter(|(k, _)| UNIT_VARS.contains(&k.as_str()));
    let warm = WarmSpec {
        unit: spec.unit.clone(),
        image: spec.image.clone(),
        sockets: spec.sockets.clone(),
        env: env.map(|(k, v)| (k.clone(), v.clone())).collect(),
    };
    let claim = ClaimSpec {
        argv: spec.argv.clone(),
        env: spec.env.clone(),
        credential: spec.credential.clone(),
        limits: spec.limits,
    };
    Ok((warm, claim))
}

/// Wait up to `limit` until `warm_ready` of the unit is true. A top
/// process that ended, or an error, ends the wait.
pub fn wait_ready(driver: &dyn UnitDriver, unit: &UnitId, limit: Duration) -> Result<(), UnitError> {
    let end = Instant::now() + limit;
    loop {
        if driver.warm_ready(unit)? {
            return Ok(());
        }
        if let Some(exit) = driver.wait(unit, Duration::ZERO)? {
            return Err(UnitError::new(
                UnitCode::UnitStartFailed,
                format!("{unit} ended before it was ready: {exit:?}"),
            ));
        }
        if Instant::now() >= end {
            return Err(UnitError::new(UnitCode::UnitStartFailed, format!("{unit} is not ready in {limit:?}")));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A name that a shell takes as a variable.
fn plain_name(k: &str) -> bool {
    let mut c = k.chars();
    c.next().is_some_and(|f| f == '_' || f.is_ascii_alphabetic()) && c.all(|x| x == '_' || x.is_ascii_alphanumeric())
}

/// One word in single quotes: the shell takes each byte as it is.
fn quote(v: &str) -> String {
    format!("'{}'", v.replace('\'', r"'\''"))
}

/// The claim as a script for `.` in `/bin/sh`: one `export` for each
/// variable and one `set --` with the argv. A name that is not a plain
/// name, or a NUL, is refused: the shell cannot carry it.
pub fn env_sh(env: &BTreeMap<String, String>, argv: &[String]) -> Result<String, UnitError> {
    let refuse = |what: String| Err(UnitError::new(UnitCode::UnitStartFailed, what));
    let mut s = String::new();
    for (k, v) in env {
        if !plain_name(k) {
            return refuse(format!("the variable {k:?} is not a plain name"));
        }
        if v.contains('\0') {
            return refuse(format!("the value of {k} holds a NUL"));
        }
        s.push_str(&format!("export {k}={}\n", quote(v)));
    }
    if argv.iter().any(|a| a.contains('\0')) {
        return refuse("a word of the argv holds a NUL".into());
    }
    s.push_str("set --");
    for a in argv {
        s.push(' ');
        s.push_str(&quote(a));
    }
    s.push('\n');
    Ok(s)
}

#[cfg(test)]
#[path = "warm_tests.rs"]
mod tests;
