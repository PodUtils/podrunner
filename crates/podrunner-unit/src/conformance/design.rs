//! The tests 18 to 27 of the unit seam: the parts of the design contract
//! (`docs/design/contracts/unit-driver.md`, tests 1, 2, 3, 6, 7, 10 and 15)
//! that the tests 1 to 17 do not cover. The tests 23 and 24 see each
//! process of the uid: run them in a test binary of their own, with no
//! other test in parallel ([`ALONE`]).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use podrunner_schema::ContainerRoute;

use super::cases::{Case, LIMIT, Unit, begin, err, root, until};
use super::{Behaviour, CANARY, ESCAPED, Fixture, MARKER, OWN, Outcome, SEEN, alive};
use crate::{ContainerSpec, ExecSpec, Exit};

/// The tests that need a process of their own (no parallel test).
pub const ALONE: [u32; 2] = [23, 24];

pub(super) const CASES: [Case; 10] = [
    Case { number: 18, name: "probe-writes-no-file", run: probe_writes_no_file },
    Case { number: 19, name: "etc-hosts-of-one-unit", run: etc_hosts_of_one_unit },
    Case { number: 20, name: "destroy-reaches-a-grandchild-in-a-container", run: grandchild_in_a_container },
    Case { number: 21, name: "a-host-scan-finds-no-credential", run: a_host_scan_finds_no_credential },
    Case { number: 22, name: "no-credential-in-the-outputs", run: no_credential_in_the_outputs },
    Case { number: 23, name: "probe-starts-no-process", run: probe_starts_no_process },
    Case { number: 24, name: "a-proc-dump-of-a-step-holds-no-credential", run: a_proc_dump_holds_no_credential },
    Case { number: 25, name: "destroy-reaches-an-orphan-with-no-tag", run: an_orphan_with_no_tag },
    Case { number: 26, name: "a-killed-shim-is-not-clean", run: a_killed_shim_is_not_clean },
    Case { number: 27, name: "a-confined-unit-reads-no-other-root", run: a_confined_unit_reads_no_other_root },
];

/// Each regular file under `dir`, with its size. A symlink is not followed.
fn files(dir: &Path) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(m) = e.path().symlink_metadata() else { continue };
            if m.is_dir() {
                todo.push(e.path());
            } else if m.is_file() {
                // Not a pipe or a socket: a read of a pipe blocks.
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

/// The processes whose parent is this process.
fn children() -> Vec<u32> {
    let me = std::process::id().to_string();
    let mut out: Vec<u32> = std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter(|p| {
            std::fs::read_to_string(format!("/proc/{p}/stat")).is_ok_and(|stat| {
                // The parent is the second field after the name in parentheses.
                stat.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().nth(1)) == Some(me.as_str())
            })
        })
        .filter(|p| alive(*p))
        .collect();
    out.sort();
    out
}

/// Design test 1, the process part: `probe` leaves no process behind.
fn probe_starts_no_process(fx: &dyn Fixture) -> Result<Outcome, String> {
    let d = fx.driver();
    let before = children();
    d.probe().map_err(|e| err("probe", e))?;
    d.probe().map_err(|e| err("probe", e))?;
    std::thread::sleep(std::time::Duration::from_millis(200));
    let new: Vec<u32> = children().into_iter().filter(|p| !before.contains(p)).collect();
    if !new.is_empty() {
        return Err(format!("probe leaves the processes {new:?}"));
    }
    Ok(Outcome::Pass)
}

/// Design test 7: a step that dumps `/proc/*/cmdline` and `/proc/*/environ`
/// finds no byte of the credential. The positive control: the dump finds a
/// blob planted in the environment of the step.
fn a_proc_dump_holds_no_credential(fx: &dyn Fixture) -> Result<Outcome, String> {
    let d = fx.driver();
    let cap = d.probe().map_err(|e| err("probe", e))?;
    if cap.container != ContainerRoute::Hooks {
        return Ok(Outcome::Skip(format!("the kind has the container route {:?}", cap.container)));
    }
    let u = begin(fx, Behaviour::Sleep)?;
    let spec = ContainerSpec { image: fx.image(), argv: fx.payload(Behaviour::Sleep), ..Default::default() };
    let c = d.container_start(&u.id, &spec).map_err(|e| err("container_start", e))?;
    let log = std::env::temp_dir().join(format!("podrunner-dump-{}-{}", std::process::id(), u.id));
    let blob = "podrunner-planted-blob-7e1d";
    let dump = ExecSpec {
        argv: ["sh", "-c", "cat /proc/[0-9]*/cmdline /proc/[0-9]*/environ 2>/dev/null; true"]
            .map(String::from)
            .to_vec(),
        env: [("PODRUNNER_PLANTED".to_string(), blob.to_string())].into(),
        log: Some(log.clone()),
        ..Default::default()
    };
    let got = d.container_exec(&u.id, &c, &dump).map_err(|e| err("container_exec", e));
    let text = std::fs::read(&log).unwrap_or_default();
    let _ = std::fs::remove_file(&log);
    match got? {
        Exit::Code(0) => {}
        other => return Err(format!("the dump gives {other:?}")),
    }
    let holds = |t: &str| text.windows(t.len()).any(|w| w == t.as_bytes());
    if !holds(blob) {
        return Err("the dump does not find the blob planted in its own environment".into());
    }
    if holds(CANARY) {
        return Err("a dump of /proc from a step finds the credential".into());
    }
    Ok(Outcome::Pass)
}

/// The pid that [`Behaviour::Escape`] wrote in the root of the unit.
fn escaped(u: &Unit) -> Result<u32, String> {
    let file = root(&u.observe()?)?.join(ESCAPED);
    let mut pid = None;
    if !until(|| {
        pid = std::fs::read_to_string(&file).ok().and_then(|t| t.trim().parse::<u32>().ok());
        pid.is_some()
    }) {
        return Err("the payload wrote no pid of its orphan".into());
    }
    pid.ok_or_else(|| "no pid".into())
}

/// Design test 3, an orphan that drops the tag of its unit (`env -i`) and
/// leaves its session (`setsid`): `destroy` ends it and counts 0 (T-063).
fn an_orphan_with_no_tag(fx: &dyn Fixture) -> Result<Outcome, String> {
    let u = begin(fx, Behaviour::Escape)?;
    let orphan = escaped(&u)?;
    // A positive control: the orphan lives and has no environment.
    // The pid file can come before the exec of `env -i`: wait for it.
    if !until(|| alive(orphan) && std::fs::read(format!("/proc/{orphan}/environ")).is_ok_and(|e| e.is_empty())) {
        return Ok(Outcome::Skip("the orphan is not seen on the host (a pid namespace)".into()));
    }
    let seen = until(|| u.observe().is_ok_and(|o| o.pids.contains(&orphan)));
    let residue = fx.driver().destroy(&u.id).map_err(|e| err("destroy", e));
    let lives = alive(orphan);
    if lives {
        crate::procs::kill(orphan, libc::SIGKILL);
    }
    let residue = residue?;
    if lives {
        return Err(format!("the orphan {orphan} lives after destroy; destroy counts {:?}", residue.processes));
    }
    if !seen {
        return Err(format!("observe does not show the orphan {orphan}"));
    }
    if !residue.processes.is_zero() {
        return Err(format!("destroy counts {:?} processes, not 0", residue.processes));
    }
    Ok(Outcome::Pass)
}

/// Design test 15: the payload kills the parent of its top process (the
/// shim), then `destroy`: it does not count 0 while the orphan lives.
fn a_killed_shim_is_not_clean(fx: &dyn Fixture) -> Result<Outcome, String> {
    let u = begin(fx, Behaviour::KillParent)?;
    let orphan = escaped(&u)?;
    if !alive(orphan) {
        return Ok(Outcome::Skip("the orphan is not seen on the host (a pid namespace)".into()));
    }
    // The kill of the parent can come just after the pid file (a parent
    // with a death signal ends its child with it); give the payload and the
    // kernel time to reparent the orphans.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let residue = fx.driver().destroy(&u.id).map_err(|e| err("destroy", e));
    let lives = alive(orphan);
    if lives {
        crate::procs::kill(orphan, libc::SIGKILL);
    }
    let residue = residue?;
    if lives && residue.processes.is_zero() {
        return Err(format!("destroy counts 0 processes, but the orphan {orphan} lives"));
    }
    Ok(Outcome::Pass)
}

/// The `confined` boundary (T-042): a unit reads no root next to its
/// own. Unit A writes its marker; unit B copies each marker of the roots
/// next to its own. The positive control: B writes and reads its own root.
fn a_confined_unit_reads_no_other_root(fx: &dyn Fixture) -> Result<Outcome, String> {
    let cap = fx.driver().probe().map_err(|e| err("probe", e))?;
    if cap.boundary < podrunner_schema::Boundary::Confined {
        return Ok(Outcome::Skip(format!("the kind claims the boundary {:?}", cap.boundary)));
    }
    let a = begin(fx, Behaviour::Marker)?;
    let ra = root(&a.observe()?)?.to_path_buf();
    if !until(|| ra.join(MARKER).exists()) {
        return Err("the first unit wrote no marker".into());
    }
    let b = begin(fx, Behaviour::ReadOthers)?;
    let rb = root(&b.observe()?)?.to_path_buf();
    if !until(|| std::fs::read_to_string(rb.join(OWN)).is_ok_and(|t| t.trim() == "own")) {
        return Err("the second unit does not write and read its own root".into());
    }
    match std::fs::read_to_string(rb.join(SEEN)) {
        Ok(t) if !t.trim().is_empty() => Err("a confined unit reads the root of another unit".into()),
        _ => Ok(Outcome::Pass),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store of podvm holds named pipes: the scan lists them as no file,
    /// so it never opens one (a read of a pipe blocks).
    #[test]
    fn the_scan_skips_a_pipe() {
        let dir = std::env::temp_dir().join(format!("podrunner-scan-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("d")).unwrap();
        std::fs::write(dir.join("d/f"), CANARY).unwrap();
        let fifo = std::ffi::CString::new(dir.join("d/p").as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: mkfifo(3) takes a path that this test owns.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert_eq!(holding(&dir, CANARY), vec![dir.join("d/f")]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
