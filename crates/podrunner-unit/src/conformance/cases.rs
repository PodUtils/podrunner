//! The tests 1 to 17 of the unit seam (`design.rs` has 18 to 22). Each starts its own units and
//! destroys them, also when it fails.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use podrunner_schema::{ContainerRoute, TreeKill, UnitId};

use super::{Behaviour, CANARY, Fixture, MARKER, Outcome, alive, canary_credential, holds_text};
use crate::{ContainerSpec, ExecSpec, Exit, Observation, Signal, StartSpec, UnitCode, UnitError, parse_pinned};

type Run = fn(&dyn Fixture) -> Result<Outcome, String>;

pub(super) struct Case {
    pub number: u32,
    pub name: &'static str,
    pub run: Run,
}

pub(super) const CASES: [Case; 17] = [
    Case { number: 1, name: "probe-is-idempotent", run: probe_is_idempotent },
    Case { number: 2, name: "prepare-refuses-a-tag", run: prepare_refuses_a_tag },
    Case { number: 3, name: "destroy-reaches-a-setsid-grandchild", run: destroy_reaches_a_setsid_grandchild },
    Case { number: 4, name: "prepare-refuses-a-foreign-platform", run: prepare_refuses_a_foreign_platform },
    Case { number: 5, name: "a-fresh-root-for-each-unit", run: a_fresh_root_for_each_unit },
    Case { number: 6, name: "credential-files-at-mode-0600", run: credential_files_at_mode_0600 },
    Case { number: 7, name: "no-credential-in-argv-or-environment", run: no_credential_in_argv_or_environment },
    Case { number: 8, name: "wait-gives-the-exit-code", run: wait_gives_the_exit_code },
    Case { number: 9, name: "term-ends-the-unit", run: term_ends_the_unit },
    Case { number: 10, name: "destroy-ends-each-process", run: destroy_ends_each_process },
    Case { number: 11, name: "destroy-removes-each-file", run: destroy_removes_each_file },
    Case { number: 12, name: "destroy-counts-the-residue", run: destroy_counts_the_residue },
    Case { number: 13, name: "container-refused-with-no-route", run: container_refused_with_no_route },
    Case { number: 14, name: "container-lifecycle", run: container_lifecycle },
    Case { number: 15, name: "destroy-is-idempotent", run: destroy_is_idempotent },
    Case { number: 16, name: "a-unit-id-in-use-is-refused", run: a_unit_id_in_use_is_refused },
    Case { number: 17, name: "a-verified-tree-kill-is-proven", run: a_verified_tree_kill_is_proven },
];

/// The time limit of each wait for an event.
pub(super) const LIMIT: Duration = Duration::from_secs(10);

pub(super) fn err(what: &str, e: UnitError) -> String {
    format!("{what}: {e}")
}

/// Poll `f` each 20 ms until it is true or the limit ends.
pub(super) fn until(mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + LIMIT;
    loop {
        if f() {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A started unit. The drop destroys it.
pub(super) struct Unit<'a> {
    pub fx: &'a dyn Fixture,
    pub id: UnitId,
}

impl Drop for Unit<'_> {
    fn drop(&mut self) {
        let _ = self.fx.driver().destroy(&self.id);
    }
}

impl Unit<'_> {
    pub fn observe(&self) -> Result<Observation, String> {
        self.fx.driver().observe(&self.id).map_err(|e| err("observe", e))
    }
}

fn spec(fx: &dyn Fixture, id: UnitId, behaviour: Behaviour) -> Result<StartSpec, String> {
    let image = fx.driver().prepare(&fx.image(), &fx.platform(), false).map_err(|e| err("prepare", e))?;
    let env = BTreeMap::from([("PODRUNNER_UNIT".to_string(), id.to_string())]);
    Ok(StartSpec { unit: id, image, argv: fx.payload(behaviour), env, credential: canary_credential(), sockets: None })
}

pub(super) fn begin(fx: &dyn Fixture, behaviour: Behaviour) -> Result<Unit<'_>, String> {
    let s = spec(fx, fx.unit(), behaviour)?;
    let unit = Unit { fx, id: s.unit.clone() };
    fx.driver().start(&s).map_err(|e| err("start", e))?;
    Ok(unit)
}

pub(super) fn root(o: &Observation) -> Result<&Path, String> {
    o.unit_root.as_deref().ok_or_else(|| "observe gives no unit root".to_string())
}

fn refused(what: &str, got: Result<impl std::fmt::Debug, UnitError>, code: UnitCode) -> Result<Outcome, String> {
    match got {
        Err(e) if e.code == code => Ok(Outcome::Pass),
        Err(e) => Err(format!("{what}: refused with {e}, not {}", code.as_str())),
        Ok(v) => Err(format!("{what}: accepted ({v:?}), not refused with {}", code.as_str())),
    }
}

fn probe_is_idempotent(fx: &dyn Fixture) -> Result<Outcome, String> {
    let d = fx.driver();
    let a = d.probe().map_err(|e| err("probe", e))?;
    let b = d.probe().map_err(|e| err("probe", e))?;
    if a != b {
        return Err("two probes give two records".into());
    }
    if a.schema != 1 || a.kind != d.kind() {
        return Err(format!("the record has schema {} and kind {:?}, not 1 and {:?}", a.schema, a.kind, d.kind()));
    }
    Ok(Outcome::Pass)
}

fn prepare_refuses_a_tag(fx: &dyn Fixture) -> Result<Outcome, String> {
    let d = fx.driver();
    let image = fx.image();
    let (name, digest) = parse_pinned(&image).map_err(|e| err("the image of the fixture", e))?;
    let got = d.prepare(&fx.image(), &fx.platform(), false).map_err(|e| err("prepare of a digest", e))?;
    if got.digest != digest {
        return Err(format!("prepare gives the digest {:?}, not {digest:?}", got.digest));
    }
    refused("prepare of a tag", d.prepare(&format!("{name}:latest"), &fx.platform(), false), UnitCode::PayloadNotPinned)
}

fn destroy_reaches_a_setsid_grandchild(fx: &dyn Fixture) -> Result<Outcome, String> {
    let u = begin(fx, Behaviour::Grandchild)?;
    let mut pids = Vec::new();
    if !until(|| u.observe().map(|o| pids = o.pids).is_ok() && pids.len() >= 3) {
        return Err(format!("the unit shows {} processes, not 3 (top, child, grandchild)", pids.len()));
    }
    let residue = fx.driver().destroy(&u.id).map_err(|e| err("destroy", e))?;
    let left: Vec<u32> = pids.iter().copied().filter(|p| alive(*p)).collect();
    if !left.is_empty() {
        return Err(format!("processes {left:?} live after destroy"));
    }
    if !residue.processes.is_zero() {
        return Err(format!("destroy counts {:?} processes, not 0", residue.processes));
    }
    Ok(Outcome::Pass)
}

fn prepare_refuses_a_foreign_platform(fx: &dyn Fixture) -> Result<Outcome, String> {
    refused(
        "prepare for a foreign platform",
        fx.driver().prepare(&fx.image(), &fx.foreign_platform(), false),
        UnitCode::ArchMismatch,
    )
}

fn a_fresh_root_for_each_unit(fx: &dyn Fixture) -> Result<Outcome, String> {
    let a = begin(fx, Behaviour::Marker)?;
    let ra = root(&a.observe()?)?.to_path_buf();
    if !until(|| ra.join(MARKER).exists()) {
        return Err("the first unit wrote no marker in its root".into());
    }
    let b = begin(fx, Behaviour::Sleep)?;
    let rb = root(&b.observe()?)?.to_path_buf();
    if ra == rb || rb.join(MARKER).exists() {
        return Err("the second unit got the root of the first".into());
    }
    Ok(Outcome::Pass)
}

fn credential_files_at_mode_0600(fx: &dyn Fixture) -> Result<Outcome, String> {
    use std::os::unix::fs::PermissionsExt;
    let u = begin(fx, Behaviour::Sleep)?;
    let o = u.observe()?;
    let dir = o.runner_root.ok_or("observe gives no runner root")?;
    for f in canary_credential().files {
        let path = dir.join(&f.name);
        let meta = std::fs::metadata(&path).map_err(|e| format!("{}: {e}", f.name))?;
        if meta.permissions().mode() & 0o777 != 0o600 {
            return Err(format!("{} has mode {:o}, not 600", f.name, meta.permissions().mode() & 0o777));
        }
        if std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", f.name))? != f.content.expose() {
            return Err(format!("{} does not hold the credential", f.name));
        }
    }
    Ok(Outcome::Pass)
}

fn no_credential_in_argv_or_environment(fx: &dyn Fixture) -> Result<Outcome, String> {
    let u = begin(fx, Behaviour::Grandchild)?;
    let mut o = Observation::default();
    if !until(|| u.observe().map(|x| o = x).is_ok() && o.pids.len() >= 3) {
        return Err("the unit shows fewer than 3 processes".into());
    }
    // A positive control: the environment of the top process is readable.
    if !holds_text(o.pids[0], u.id.as_str()) {
        return Err("the environment of the top process cannot be read (PODRUNNER_UNIT is not seen)".into());
    }
    if let Some(p) = o.pids.iter().find(|p| holds_text(**p, CANARY)) {
        return Err(format!("process {p} holds the credential in its argv or environment"));
    }
    if format!("{o:?}").contains(CANARY) {
        return Err("observe shows the credential".into());
    }
    Ok(Outcome::Pass)
}

fn wait_gives_the_exit_code(fx: &dyn Fixture) -> Result<Outcome, String> {
    let u = begin(fx, Behaviour::Exit(7))?;
    match fx.driver().wait(&u.id, LIMIT).map_err(|e| err("wait", e))? {
        Some(Exit::Code(7)) => Ok(Outcome::Pass),
        other => Err(format!("wait gives {other:?}, not exit code 7")),
    }
}

fn term_ends_the_unit(fx: &dyn Fixture) -> Result<Outcome, String> {
    let u = begin(fx, Behaviour::Sleep)?;
    let d = fx.driver();
    d.signal(&u.id, Signal::Term).map_err(|e| err("signal", e))?;
    match d.wait(&u.id, LIMIT).map_err(|e| err("wait", e))? {
        Some(_) => Ok(Outcome::Pass),
        None => Err("the unit runs after TERM".into()),
    }
}

fn destroy_ends_each_process(fx: &dyn Fixture) -> Result<Outcome, String> {
    let u = begin(fx, Behaviour::Sleep)?;
    let mut pids = Vec::new();
    if !until(|| u.observe().map(|o| pids = o.pids).is_ok() && !pids.is_empty()) {
        return Err("the unit shows no process".into());
    }
    fx.driver().destroy(&u.id).map_err(|e| err("destroy", e))?;
    match pids.iter().find(|p| alive(**p)) {
        Some(p) => Err(format!("process {p} lives after destroy")),
        None => Ok(Outcome::Pass),
    }
}

fn destroy_removes_each_file(fx: &dyn Fixture) -> Result<Outcome, String> {
    let u = begin(fx, Behaviour::Marker)?;
    let o = u.observe()?;
    let r = root(&o)?.to_path_buf();
    if !until(|| r.join(MARKER).exists()) {
        return Err("the unit wrote no marker in its root".into());
    }
    fx.driver().destroy(&u.id).map_err(|e| err("destroy", e))?;
    for p in [Some(r), o.runner_root].into_iter().flatten() {
        if p.exists() {
            return Err(format!("{} exists after destroy", p.display()));
        }
    }
    Ok(Outcome::Pass)
}

fn destroy_counts_the_residue(fx: &dyn Fixture) -> Result<Outcome, String> {
    let u = begin(fx, Behaviour::Sleep)?;
    let r = fx.driver().destroy(&u.id).map_err(|e| err("destroy", e))?;
    if !r.is_clean() {
        return Err(format!("destroy leaves {r:?}, not 0 processes and 0 files"));
    }
    Ok(Outcome::Pass)
}

fn container_refused_with_no_route(fx: &dyn Fixture) -> Result<Outcome, String> {
    let cap = fx.driver().probe().map_err(|e| err("probe", e))?;
    if cap.container != ContainerRoute::None {
        return Ok(Outcome::Skip(format!("the kind has the container route {:?}", cap.container)));
    }
    let u = begin(fx, Behaviour::Sleep)?;
    let c = ContainerSpec { image: fx.image(), ..Default::default() };
    refused("container_start", fx.driver().container_start(&u.id, &c), UnitCode::ContainerNotSupported)
}

fn container_lifecycle(fx: &dyn Fixture) -> Result<Outcome, String> {
    let d = fx.driver();
    let cap = d.probe().map_err(|e| err("probe", e))?;
    if cap.container != ContainerRoute::Hooks {
        return Ok(Outcome::Skip(format!("the kind has the container route {:?}", cap.container)));
    }
    let u = begin(fx, Behaviour::Sleep)?;
    let spec = ContainerSpec { image: fx.image(), argv: fx.payload(Behaviour::Sleep), ..Default::default() };
    let c = d.container_start(&u.id, &spec).map_err(|e| err("container_start", e))?;
    if !d.container_inspect(&u.id, &c).map_err(|e| err("container_inspect", e))?.running {
        return Err("the container does not run after container_start".into());
    }
    let exec = ExecSpec { argv: fx.payload(Behaviour::Exit(0)), ..Default::default() };
    match d.container_exec(&u.id, &c, &exec).map_err(|e| err("container_exec", e))? {
        Exit::Code(0) => {}
        other => return Err(format!("container_exec gives {other:?}, not exit code 0")),
    }
    // The design test 8: the command sees its environment and its
    // working directory.
    let probe = ExecSpec {
        argv: ["sh", "-c", "test \"$PODRUNNER_PROBE\" = seen && test \"$(pwd)\" = /tmp"].map(String::from).to_vec(),
        env: [("PODRUNNER_PROBE".to_string(), "seen".to_string())].into(),
        workdir: Some("/tmp".into()),
        ..Default::default()
    };
    match d.container_exec(&u.id, &c, &probe).map_err(|e| err("container_exec", e))? {
        Exit::Code(0) => {}
        other => return Err(format!("container_exec with an environment and a workdir gives {other:?}")),
    }
    refused(
        "container_exec with stdin",
        d.container_exec(&u.id, &c, &ExecSpec { stdin: true, ..exec }),
        UnitCode::ContainerOptionRefused,
    )?;
    d.container_logs(&u.id, &c).map_err(|e| err("container_logs", e))?;
    d.container_stop(&u.id, &c).map_err(|e| err("container_stop", e))?;
    match d.container_inspect(&u.id, &c) {
        Ok(s) if s.running => Err("the container runs after container_stop".into()),
        _ => Ok(Outcome::Pass),
    }
}

fn destroy_is_idempotent(fx: &dyn Fixture) -> Result<Outcome, String> {
    let u = begin(fx, Behaviour::Sleep)?;
    let d = fx.driver();
    d.destroy(&u.id).map_err(|e| err("the first destroy", e))?;
    let r = d.destroy(&u.id).map_err(|e| err("the second destroy", e))?;
    if !r.is_clean() {
        return Err(format!("the second destroy counts {r:?}"));
    }
    Ok(Outcome::Pass)
}

fn a_unit_id_in_use_is_refused(fx: &dyn Fixture) -> Result<Outcome, String> {
    let u = begin(fx, Behaviour::Sleep)?;
    let again = spec(fx, u.id.clone(), Behaviour::Sleep)?;
    refused("a second start of one unit id", fx.driver().start(&again), UnitCode::UnitInUse)
}

fn a_verified_tree_kill_is_proven(fx: &dyn Fixture) -> Result<Outcome, String> {
    let cap = fx.driver().probe().map_err(|e| err("probe", e))?;
    if cap.tree_kill != TreeKill::Verified {
        return Ok(Outcome::Pass);
    }
    destroy_reaches_a_setsid_grandchild(fx)
        .map_err(|why| format!("probe claims a verified tree kill, but test 3 fails: {why}"))
}
