//! The tests 28 to 32 of the unit seam: warm units (T-487). A kind whose
//! `warm` is `unsupported` skips each. [`super::run_warm`] runs the tests
//! 1 to 32 through `warm` and `claim` too.

use std::time::Duration;

use super::cases::{Case, Unit, begin, err, host_pids, refused, root, spec, until};
use super::design::holding;
use super::{Behaviour, CANARY, Fixture, MARKER, Outcome, alive, canary_credential, holds_text};
use crate::warm::{ClaimSpec, split, wait_ready};
use crate::{Exit, UnitCode};

pub(super) const CASES: [Case; 5] = [
    Case { number: 28, name: "a-warm-unit-holds-no-credential", run: a_warm_unit_holds_no_credential },
    Case { number: 29, name: "a-warm-unit-is-claimed-once", run: a_warm_unit_is_claimed_once },
    Case { number: 30, name: "an-unclaimed-warm-unit-destroys-clean", run: an_unclaimed_warm_unit_destroys_clean },
    Case { number: 31, name: "a-claim-runs-its-own-argv-and-env", run: a_claim_runs_its_own_argv_and_env },
    Case { number: 32, name: "a-warm-unit-has-no-file-of-an-ended-unit", run: no_file_of_an_ended_unit },
];

/// The time that a warm unit may take to boot (a guest boots in about
/// 30 s).
pub(super) const READY: Duration = Duration::from_secs(240);

/// A warm unit that waits for its claim, and the claim of a payload with
/// `behaviour`; `Err`: the kind has no warm unit (a skip).
type Warmed<'a> = Result<(Unit<'a>, ClaimSpec), Outcome>;

fn warm_unit(fx: &dyn Fixture, behaviour: Behaviour) -> Result<Warmed<'_>, String> {
    let s = spec(fx, fx.unit(), behaviour)?;
    let (w, c) = split(&s).map_err(|e| err("split", e))?;
    let unit = Unit { fx, id: w.unit.clone() };
    match fx.driver().warm(&w) {
        Err(e) if e.code == UnitCode::Unsupported => return Ok(Err(Outcome::Skip("the kind has no warm unit".into()))),
        Err(e) => return Err(err("warm", e)),
        Ok(()) => {}
    }
    wait_ready(fx.driver(), &unit.id, READY).map_err(|e| err("warm_ready", e))?;
    Ok(Ok((unit, c)))
}

/// Each credential file in the runner root of the unit, at mode 0600.
fn credential_files_there(u: &Unit) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let Ok(Some(dir)) = u.observe().map(|o| o.runner_root) else { return false };
    canary_credential().files.iter().all(|f| {
        std::fs::metadata(dir.join(&f.name)).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o777 == 0o600)
    })
}

/// The credential in the files under the roots, the processes or the
/// observation of a unit that waits for its claim.
fn credential_seen(fx: &dyn Fixture, u: &Unit, scan_state: bool) -> Result<(), String> {
    let o = u.observe()?;
    let state = fx.state_dir().filter(|_| scan_state);
    let dirs = state.into_iter().chain(o.unit_root.clone()).chain(o.runner_root.clone());
    if let Some(p) = dirs.flat_map(|d| holding(&d, CANARY)).next() {
        return Err(format!("{} holds the credential before the claim of {}", p.display(), u.id));
    }
    if let Some(p) = o.pids.iter().find(|p| holds_text(**p, CANARY)) {
        return Err(format!("process {p} holds the credential before the claim of {}", u.id));
    }
    if format!("{o:?}").contains(CANARY) {
        return Err(format!("observe shows the credential before the claim of {}", u.id));
    }
    Ok(())
}

/// Before its claim, no file of the state directory or of the roots, no
/// process and no observation of a warm unit holds the credential; after
/// the claim, the files are there at mode 0600 (a positive control). A
/// second warm unit, made after that claim, holds none of it either.
fn a_warm_unit_holds_no_credential(fx: &dyn Fixture) -> Result<Outcome, String> {
    let (u, claim) = match warm_unit(fx, Behaviour::Sleep)? {
        Ok(x) => x,
        Err(skip) => return Ok(skip),
    };
    credential_seen(fx, &u, true)?;
    fx.driver().claim(&u.id, &claim).map_err(|e| err("claim", e))?;
    if !until(|| credential_files_there(&u)) {
        return Err("after the claim, the runner root has no credential file at mode 0600".into());
    }
    let (v, _) = match warm_unit(fx, Behaviour::Sleep)? {
        Ok(x) => x,
        Err(skip) => return Ok(skip),
    };
    credential_seen(fx, &v, false)?;
    Ok(Outcome::Pass)
}

/// A second claim, and a claim of a unit of `start`, are `unit-in-use`; a
/// claim of an unknown id is `unknown-unit`.
fn a_warm_unit_is_claimed_once(fx: &dyn Fixture) -> Result<Outcome, String> {
    let (u, claim) = match warm_unit(fx, Behaviour::Sleep)? {
        Ok(x) => x,
        Err(skip) => return Ok(skip),
    };
    let d = fx.driver();
    d.claim(&u.id, &claim).map_err(|e| err("the first claim", e))?;
    refused("a second claim", d.claim(&u.id, &claim), UnitCode::UnitInUse)?;
    let cold = begin(fx, Behaviour::Sleep)?;
    refused("a claim of a unit of start", d.claim(&cold.id, &claim), UnitCode::UnitInUse)?;
    refused("a claim of an unknown unit", d.claim(&fx.unit(), &claim), UnitCode::UnknownUnit)
}

/// A warm unit that got no claim ends with no process, no file and a
/// residue of 0.
fn an_unclaimed_warm_unit_destroys_clean(fx: &dyn Fixture) -> Result<Outcome, String> {
    let (u, _) = match warm_unit(fx, Behaviour::Sleep)? {
        Ok(x) => x,
        Err(skip) => return Ok(skip),
    };
    let o = u.observe()?;
    if o.pids.is_empty() {
        return Err("the warm unit shows no process".into());
    }
    let residue = fx.driver().destroy(&u.id).map_err(|e| err("destroy", e))?;
    if !residue.is_clean() {
        return Err(format!("destroy counts {residue:?}, not 0 and 0"));
    }
    if let Some(p) = o.pids.iter().find(|p| alive(**p)) {
        return Err(format!("process {p} lives after destroy"));
    }
    match o.unit_root.iter().chain(o.runner_root.iter()).find(|p| p.exists()) {
        Some(p) => Err(format!("{} is there after destroy", p.display())),
        None => Ok(Outcome::Pass),
    }
}

/// The variable of the claim that test 31 looks for.
const CLAIM_VAR: &str = "PODRUNNER_CLAIM_PROBE";

/// The claim runs its argv (`wait` gives its exit code) with its
/// environment (where the processes of the unit are on the host).
fn a_claim_runs_its_own_argv_and_env(fx: &dyn Fixture) -> Result<Outcome, String> {
    let (u, claim) = match warm_unit(fx, Behaviour::Exit(7))? {
        Ok(x) => x,
        Err(skip) => return Ok(skip),
    };
    // A guest, or an incus instance (its processes belong to incusd),
    // shows no process of the unit to read.
    let on_host = host_pids(fx) != 1 && !u.observe()?.pids.is_empty();
    let d = fx.driver();
    d.claim(&u.id, &claim).map_err(|e| err("claim", e))?;
    match d.wait(&u.id, READY).map_err(|e| err("wait", e))? {
        Some(Exit::Code(7)) => {}
        other => return Err(format!("wait gives {other:?}, not the exit code 7 of the argv of the claim")),
    }
    if !on_host {
        return Ok(Outcome::Pass);
    }
    let (v, mut claim) = match warm_unit(fx, Behaviour::Sleep)? {
        Ok(x) => x,
        Err(skip) => return Ok(skip),
    };
    let value = format!("claim-of-{}", v.id);
    claim.env.insert(CLAIM_VAR.into(), value.clone());
    d.claim(&v.id, &claim).map_err(|e| err("claim", e))?;
    let seen = until(|| v.observe().is_ok_and(|o| o.pids.iter().any(|p| holds_text(*p, &value))));
    if !seen {
        return Err(format!("no process of the unit has {CLAIM_VAR} of the claim"));
    }
    Ok(Outcome::Pass)
}

/// A warm unit that waited while another unit wrote a file and ended gets
/// a root with no file of that unit: no reused root, no reused name.
fn no_file_of_an_ended_unit(fx: &dyn Fixture) -> Result<Outcome, String> {
    let (w, claim) = match warm_unit(fx, Behaviour::Sleep)? {
        Ok(x) => x,
        Err(skip) => return Ok(skip),
    };
    let a = begin(fx, Behaviour::Marker)?;
    let ra = root(&a.observe()?)?.to_path_buf();
    if !until(|| ra.join(MARKER).exists()) {
        return Err("the ended unit wrote no marker in its root".into());
    }
    fx.driver().destroy(&a.id).map_err(|e| err("destroy", e))?;
    drop(a);
    fx.driver().claim(&w.id, &claim).map_err(|e| err("claim", e))?;
    let rw = root(&w.observe()?)?.to_path_buf();
    if rw == ra || rw.join(MARKER).exists() {
        return Err("the warm unit got the root or a file of the unit that ended".into());
    }
    Ok(Outcome::Pass)
}
