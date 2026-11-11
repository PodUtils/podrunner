//! The tests 18 to 22 of the unit seam: the parts of the design contract
//! (`docs/design/contracts/unit-driver.md`, tests 1, 2, 3, 6 and 10) that
//! the tests 1 to 17 do not cover.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use podrunner_schema::ContainerRoute;

use super::cases::{Case, LIMIT, Unit, begin, err, root, until};
use super::{Behaviour, CANARY, Fixture, Outcome, alive};
use crate::ContainerSpec;

pub(super) const CASES: [Case; 5] = [
    Case { number: 18, name: "probe-writes-no-file", run: probe_writes_no_file },
    Case { number: 19, name: "etc-hosts-of-one-unit", run: etc_hosts_of_one_unit },
    Case { number: 20, name: "destroy-reaches-a-grandchild-in-a-container", run: grandchild_in_a_container },
    Case { number: 21, name: "a-host-scan-finds-no-credential", run: a_host_scan_finds_no_credential },
    Case { number: 22, name: "no-credential-in-the-outputs", run: no_credential_in_the_outputs },
];

/// Each file under `dir`, with its size. A symlink is listed, not followed.
fn files(dir: &Path) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(m) = e.path().symlink_metadata() else { continue };
            if m.is_dir() {
                todo.push(e.path());
            } else {
                out.push((e.path(), m.len()));
            }
        }
    }
    out.sort();
    out
}

/// The files under `dir` that hold `text`.
fn holding(dir: &Path, text: &str) -> Vec<PathBuf> {
    files(dir)
        .into_iter()
        .filter(|(p, _)| std::fs::read(p).is_ok_and(|b| b.windows(text.len()).any(|w| w == text.as_bytes())))
        .map(|(p, _)| p)
        .collect()
}

/// Design test 1: `probe` changes no file of the state of the driver.
fn probe_writes_no_file(fx: &dyn Fixture) -> Result<Outcome, String> {
    let d = fx.driver();
    d.probe().map_err(|e| err("probe", e))?;
    let Some(dir) = fx.state_dir() else {
        return Ok(Outcome::Skip("the fixture names no state directory".into()));
    };
    let before = files(&dir);
    d.probe().map_err(|e| err("probe", e))?;
    d.probe().map_err(|e| err("probe", e))?;
    let after = files(&dir);
    if before != after {
        let new: Vec<_> = after.iter().filter(|f| !before.contains(f)).map(|(p, _)| p.display().to_string()).collect();
        return Err(format!("probe changes the state directory: {new:?}"));
    }
    Ok(Outcome::Pass)
}

/// Design test 2: a name in `/etc/hosts` of one unit is not in that of
/// another.
fn etc_hosts_of_one_unit(fx: &dyn Fixture) -> Result<Outcome, String> {
    let a = begin(fx, Behaviour::Sleep)?;
    let b = begin(fx, Behaviour::Sleep)?;
    let (ra, rb) = (root(&a.observe()?)?.to_path_buf(), root(&b.observe()?)?.to_path_buf());
    let name = format!("podrunner-name-of-{}", a.id);
    std::fs::create_dir_all(ra.join("etc")).map_err(|e| format!("etc of the first unit: {e}"))?;
    let mut f = OpenOptions::new()
        .append(true)
        .create(true)
        .open(ra.join("etc/hosts"))
        .map_err(|e| format!("/etc/hosts of the first unit: {e}"))?;
    writeln!(f, "127.0.0.2 {name}").map_err(|e| format!("/etc/hosts of the first unit: {e}"))?;
    // A positive control: the name is in the file of the first unit.
    if !holding(&ra.join("etc"), &name).iter().any(|p| p.ends_with("hosts")) {
        return Err("the name is not seen in /etc/hosts of the first unit".into());
    }
    match std::fs::read_to_string(rb.join("etc/hosts")) {
        Ok(t) if t.contains(&name) => Err("/etc/hosts of the second unit holds the name of the first".into()),
        _ => Ok(Outcome::Pass),
    }
}

/// Design test 3, the container part: a `setsid` grandchild in a container
/// ends at `destroy`, within the time limit.
fn grandchild_in_a_container(fx: &dyn Fixture) -> Result<Outcome, String> {
    let d = fx.driver();
    let cap = d.probe().map_err(|e| err("probe", e))?;
    if cap.container != ContainerRoute::Hooks {
        return Ok(Outcome::Skip(format!("the kind has the container route {:?}", cap.container)));
    }
    let u: Unit = begin(fx, Behaviour::Sleep)?;
    let mut top = Vec::new();
    if !until(|| u.observe().map(|o| top = o.pids).is_ok() && !top.is_empty()) {
        return Err("the unit shows no process".into());
    }
    let spec = ContainerSpec { image: fx.image(), argv: fx.payload(Behaviour::Grandchild), ..Default::default() };
    d.container_start(&u.id, &spec).map_err(|e| err("container_start", e))?;
    let mut pids = Vec::new();
    if !until(|| u.observe().map(|o| pids = o.pids).is_ok() && pids.len() >= top.len() + 3) {
        return Err(format!(
            "the unit shows {} processes, not {} (the container tree is not seen)",
            pids.len(),
            top.len() + 3
        ));
    }
    let start = Instant::now();
    let residue = d.destroy(&u.id).map_err(|e| err("destroy", e))?;
    if start.elapsed() > LIMIT {
        return Err(format!("destroy takes {:?}, more than {LIMIT:?}", start.elapsed()));
    }
    let left: Vec<u32> = pids.iter().copied().filter(|p| alive(*p)).collect();
    if !left.is_empty() {
        return Err(format!("processes {left:?} of the container live after destroy"));
    }
    if !residue.processes.is_zero() {
        return Err(format!("destroy counts {:?} processes, not 0", residue.processes));
    }
    Ok(Outcome::Pass)
}

/// Design test 6: after `destroy`, no file of the state directory holds a
/// byte of the credential. The positive control: the scan finds the
/// credential files before `destroy`.
fn a_host_scan_finds_no_credential(fx: &dyn Fixture) -> Result<Outcome, String> {
    let u = begin(fx, Behaviour::Sleep)?;
    let Some(dir) = fx.state_dir() else {
        return Ok(Outcome::Skip("the fixture names no state directory".into()));
    };
    if holding(&dir, CANARY).is_empty() {
        return Err("the scan does not find the credential files of a running unit".into());
    }
    fx.driver().destroy(&u.id).map_err(|e| err("destroy", e))?;
    match holding(&dir, CANARY).first() {
        Some(p) => Err(format!("{} holds the credential after destroy", p.display())),
        None => Ok(Outcome::Pass),
    }
}

/// Design test 10: the probe record (`boundary_proof` is free text), the
/// observation and the residue hold no byte of the credential.
fn no_credential_in_the_outputs(fx: &dyn Fixture) -> Result<Outcome, String> {
    let d = fx.driver();
    let u = begin(fx, Behaviour::Sleep)?;
    let mut outputs = vec![
        ("probe", format!("{:?}", d.probe().map_err(|e| err("probe", e))?)),
        ("observe", format!("{:?}", u.observe()?)),
    ];
    outputs.push(("destroy", format!("{:?}", d.destroy(&u.id).map_err(|e| err("destroy", e))?)));
    // A positive control: the scan finds the canary in a planted output.
    if !format!("{:?}", Some(CANARY)).contains(CANARY) {
        return Err("the scan does not find a planted canary".into());
    }
    match outputs.iter().find(|(_, o)| o.contains(CANARY)) {
        Some((what, _)) => Err(format!("{what} gives the credential")),
        None => Ok(Outcome::Pass),
    }
}
