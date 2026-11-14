//! The guest floor self-test (GitHub #36). `untrusted` jobs need boundary
//! `guest`, a separate kernel, and a driver only declares its boundary. So
//! slots accepts `guest` only from a kind on the allowlist (`guest_kinds`,
//! default `microvm`), after a self-test unit proves it; else the claim
//! drops to `none`.
//!
//! The test unit writes what it sees into its working directory: the boot
//! id of its kernel, its network interfaces other than `lo`, whether a
//! canary file that slots planted on the host is visible, whether the
//! process of slots is visible, and whether the process and the file of a
//! first unit that still runs are visible. The host and the unit share no
//! boot id and no kernel version string: a system container of LXC or incus
//! gets a boot id of its own, but it reads the `/proc/version` of the host
//! kernel. A guest that boots the same kernel build as the host fails too:
//! the test fails safe.

use std::path::Path;
use std::time::Duration;

use podrunner_schema::{Boundary, Capability, UnitId};
use podrunner_unit::{StartSpec, UnitDriver};

/// The file that the test unit writes in its working directory.
const RESULT: &str = "podrunner-guest-test";

/// Why a claim of `guest` failed, or `Ok` when it holds.
pub fn self_test(
    driver: &dyn UnitDriver,
    payload: &str,
    platform: &str,
    units: (UnitId, UnitId),
    scratch: &Path,
) -> Result<(), String> {
    let (text, reached) = observe(driver, payload, platform, units, scratch)?;
    if reached {
        return Err("the unit reached a listener on the loopback of the host".into());
    }
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default().trim().to_string();
    judge(&text, &Host { boot_id: read("/proc/sys/kernel/random/boot_id"), version: read("/proc/version") })
}

/// The lines of the test unit, and whether it reached the listener on the
/// loopback of the host.
pub(crate) fn observe(
    driver: &dyn UnitDriver,
    payload: &str,
    platform: &str,
    (first, unit): (UnitId, UnitId),
    scratch: &Path,
) -> Result<(String, bool), String> {
    // A first unit that runs during the test: its marker is in its argv and
    // in a file of its root. The test unit builds the marker from two parts,
    // so that the marker is in no argv of the test unit.
    let mark = format!("{}", std::process::id());
    let image = driver.prepare(payload, platform, false).map_err(|e| format!("prepare: {e}"))?;
    let argv = vec!["sh".into(), "-c".into(), format!(": > podrunner-first-{mark}; sleep 60; :")];
    let spec = StartSpec {
        unit: first.clone(),
        image,
        argv,
        env: Default::default(),
        credential: Default::default(),
        sockets: None,
        mounts: Vec::new(),
    };
    driver.start(&spec).map_err(|e| format!("start the first unit: {e}"))?;
    let first_root = driver.observe(&first).ok().and_then(|o| o.unit_root);
    let verdict = second(driver, payload, platform, unit, scratch, &mark, first_root.as_deref());
    let _ = driver.destroy(&first);
    verdict
}

/// The test unit, while the first unit runs.
fn second(
    driver: &dyn UnitDriver,
    payload: &str,
    platform: &str,
    unit: UnitId,
    scratch: &Path,
    mark: &str,
    first_root: Option<&Path>,
) -> Result<(String, bool), String> {
    let first_file = first_root.map(|r| r.display().to_string()).unwrap_or_else(|| "/nonexistent".into());
    let canary = scratch.join(format!("podrunner-guest-canary-{}", std::process::id()));
    std::fs::write(&canary, "canary").map_err(|e| format!("the canary: {}", e.kind()))?;
    // A listener on the loopback of the host: a unit that reaches it shares
    // the network of the host. The unit tries with the tool it has.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| format!("the listener: {}", e.kind()))?;
    listener.set_nonblocking(true).map_err(|e| format!("the listener: {}", e.kind()))?;
    let port = listener.local_addr().map_err(|e| format!("the listener: {}", e.kind()))?.port();
    let me = std::process::id();
    let script = format!(
        "{{ echo boot_id=$(cat /proc/sys/kernel/random/boot_id); \
         echo kernel=$(cat /proc/version); \
         echo net=$(ls /sys/class/net 2>/dev/null | grep -vx lo | tr '\\n' ' '); \
         if [ -e '{canary}' ]; then echo canary=visible; else echo canary=absent; fi; \
         if [ -d /proc/{me} ] && grep -q podrunner /proc/{me}/cmdline 2>/dev/null; then echo hostpid=visible; else echo hostpid=absent; fi; \
         f=podrunner-first; f=$f-{mark}; p=absent; \
         for c in /proc/[0-9]*/cmdline; do case \"$(tr '\\0' ' ' < $c 2>/dev/null)\" in *\"$f\"*) p=visible;; esac; done; echo firstproc=$p; \
         if [ -e '{first_file}'/\"$f\" ]; then echo firstfile=visible; else echo firstfile=absent; fi; \
         }} > {RESULT}; \
         (command -v nc >/dev/null && nc -w 1 127.0.0.1 {port} </dev/null) >/dev/null 2>&1; \
         (command -v bash >/dev/null && bash -c 'exec 3<>/dev/tcp/127.0.0.1/{port}') >/dev/null 2>&1; true",
        canary = canary.display()
    );
    let result = run(driver, payload, platform, unit, &script);
    let _ = std::fs::remove_file(&canary);
    let reached = listener.accept().is_ok();
    Ok((result?, reached))
}

/// Start the test unit, wait for its end, and read its result file.
fn run(driver: &dyn UnitDriver, payload: &str, platform: &str, unit: UnitId, script: &str) -> Result<String, String> {
    let image = driver.prepare(payload, platform, false).map_err(|e| format!("prepare: {e}"))?;
    let argv = vec!["sh".into(), "-c".into(), script.into()];
    let spec = StartSpec {
        unit: unit.clone(),
        image,
        argv,
        env: Default::default(),
        credential: Default::default(),
        sockets: None,
        mounts: Vec::new(),
    };
    driver.start(&spec).map_err(|e| format!("start: {e}"))?;
    let root = driver.observe(&unit).ok().and_then(|o| o.unit_root);
    let ended = driver.wait(&unit, Duration::from_secs(60)).map_err(|e| format!("wait: {e}"));
    let text = root.map(|r| std::fs::read_to_string(r.join(RESULT)));
    let _ = driver.destroy(&unit);
    ended?.ok_or("the test unit did not end in 60 s")?;
    match text {
        Some(Ok(t)) => Ok(t),
        _ => Err("the test unit wrote no result the host can read".into()),
    }
}

/// What the host reads of its own kernel.
#[derive(Debug, Clone, Default)]
pub struct Host {
    pub boot_id: String,
    pub version: String,
}

/// The verdict on the lines of the test unit.
pub fn judge(text: &str, host: &Host) -> Result<(), String> {
    let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).map(str::trim).unwrap_or("");
    let boot = get("boot_id");
    if boot.is_empty() || boot == host.boot_id {
        return Err("the unit runs on the kernel of the host (the same boot id)".into());
    }
    // The shell of the unit collapses each run of blanks: compare words.
    let kernel = get("kernel");
    if kernel.is_empty() || kernel.split_whitespace().eq(host.version.split_whitespace()) {
        return Err("the unit runs on the kernel of the host (the same kernel version)".into());
    }
    if !get("net").is_empty() {
        return Err(format!("the unit has network interfaces: {}", get("net")));
    }
    if get("canary") != "absent" {
        return Err("the unit sees a file of the host".into());
    }
    if get("hostpid") != "absent" {
        return Err("the unit sees a process of the host".into());
    }
    if get("firstproc") != "absent" {
        return Err("the unit sees a process of another unit".into());
    }
    if get("firstfile") != "absent" {
        return Err("the unit sees a file of another unit".into());
    }
    Ok(())
}

/// The test runs again after this time.
pub const RETEST_MS: u64 = 24 * 3600 * 1000;

/// The identity of a driver file: a new install changes it.
pub type Stamp = (u64, u64, i64);

/// The inode, the size and the time of change of `path`.
pub fn stamp(path: &Path) -> Option<Stamp> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| (m.ino(), m.size(), m.ctime()))
}

/// A kind that claims `guest`: its claim, and when slots tests it again
/// (each day, and when its driver file changes).
#[derive(Debug, Clone)]
pub struct Retest {
    pub claim: Capability,
    pub allowed: bool,
    pub payload: String,
    pub platform: String,
    pub last_ms: u64,
    pub driver: Option<std::path::PathBuf>,
    pub stamp: Option<Stamp>,
}

impl Retest {
    pub fn due(&self, now_ms: u64) -> bool {
        let changed = self.driver.as_deref().is_some_and(|d| stamp(d) != self.stamp);
        changed || now_ms.saturating_sub(self.last_ms) >= RETEST_MS
    }
}

/// The unit ids of the self-test: above each id that a job gets.
pub fn test_ids() -> (UnitId, UnitId) {
    let id = |n: &str| UnitId::parse(n).expect("a unit id");
    (id("u-999999997"), id("u-999999998"))
}

/// The record of a kind after the test: a claim of `guest` that is not on
/// the allowlist, or that fails, drops to `none`.
pub fn settle(mut cap: Capability, allowed: bool, verdict: Result<(), String>) -> (Capability, Option<String>) {
    if cap.boundary != Boundary::Guest {
        return (cap, None);
    }
    let why = if !allowed { Some("the kind is not on guest_kinds".to_string()) } else { verdict.err() };
    match why {
        Some(why) => {
            cap.boundary = Boundary::None;
            cap.boundary_proof = Some("guest-self-test-failed".into());
            (cap, Some(why))
        }
        None => {
            cap.boundary_proof = Some("guest-self-test".into());
            (cap, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verdict_needs_each_line() {
        let good = "boot_id=b2\nkernel=Linux version 6.1 (b) #2\nnet=\ncanary=absent\nhostpid=absent\nfirstproc=absent\nfirstfile=absent\n";
        let host = Host { boot_id: "b1".into(), version: "Linux version  6.12 (a) #1".into() };
        assert_eq!(judge(good, &host), Ok(()));
        for (from, to, why) in [
            ("boot_id=b2", "boot_id=b1", "same boot id"),
            // A system container: its own boot id, the kernel of the host.
            ("6.1 (b) #2", "6.12 (a) #1", "same kernel version"),
            ("kernel=Linux version 6.1 (b) #2\n", "", "same kernel version"),
            ("net=\n", "net=eth0\n", "network"),
            ("canary=absent", "canary=visible", "file of the host"),
            ("hostpid=absent", "hostpid=visible", "process of the host"),
            ("firstproc=absent", "firstproc=visible", "process of another unit"),
            ("firstfile=absent", "firstfile=visible", "file of another unit"),
            ("firstfile=absent\n", "", "file of another unit"),
            (good, "", "same boot id"),
        ] {
            let bad = good.replace(from, to);
            assert!(judge(&bad, &host).unwrap_err().contains(why), "{bad}");
        }
    }
}
