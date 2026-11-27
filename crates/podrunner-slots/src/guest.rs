//! The guest floor self-test (GitHub #36). `untrusted` jobs need boundary
//! `guest`, a separate kernel, and a driver only declares its boundary. So
//! slots accepts `guest` only from a kind on the allowlist (`guest_kinds`,
//! default `microvm`), after a self-test unit proves it; else the claim
//! drops to `none`.
//!
//! The test unit writes what it sees into its working directory: the boot
//! id of its kernel, its network interfaces that have a device (a NIC of
//! the machine; see [`net_lines`]), whether a
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
use podrunner_unit::{Exit, StartSpec, UnitDriver};

/// The file that the test unit writes in its working directory.
const RESULT: &str = "podrunner-guest-test";

/// Why a claim of `guest` does not hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fail {
    /// The test unit ran, and its result shows that the boundary does not
    /// hold. The test runs again after [`RETEST_MS`].
    Verdict(String),
    /// The test gave no verdict: a unit did not start, did not end in time,
    /// or wrote no result. The test runs again after [`next_ms`].
    NoVerdict(String),
}

impl Fail {
    pub fn no_verdict(&self) -> bool {
        matches!(self, Fail::NoVerdict(_))
    }
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fail::Verdict(why) => f.write_str(why),
            Fail::NoVerdict(why) => write!(f, "no verdict: {why}"),
        }
    }
}

/// Why a claim of `guest` failed, or `Ok` when it holds. A test with no
/// verdict runs once more at once: one cold start that fails does not take
/// the kind out.
pub fn self_test(
    driver: &dyn UnitDriver,
    payload: &str,
    platform: &str,
    units: (UnitId, UnitId),
    scratch: &Path,
) -> Result<(), Fail> {
    match attempt(driver, payload, platform, units.clone(), scratch) {
        Err(Fail::NoVerdict(_)) => attempt(driver, payload, platform, units, scratch).map_err(|e| match e {
            Fail::NoVerdict(why) => Fail::NoVerdict(format!("two tries, the last: {why}")),
            verdict => verdict,
        }),
        verdict => verdict,
    }
}

/// One run of the test.
fn attempt(
    driver: &dyn UnitDriver,
    payload: &str,
    platform: &str,
    units: (UnitId, UnitId),
    scratch: &Path,
) -> Result<(), Fail> {
    let (text, reached) = observe(driver, payload, platform, units, scratch)?;
    if reached {
        return Err(Fail::Verdict("the unit reached a listener on the loopback of the host".into()));
    }
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default().trim().to_string();
    judge(&text, &Host { boot_id: read("/proc/sys/kernel/random/boot_id"), version: read("/proc/version") })
        .map_err(Fail::Verdict)
}

/// The lines of the test unit, and whether it reached the listener on the
/// loopback of the host.
pub(crate) fn observe(
    driver: &dyn UnitDriver,
    payload: &str,
    platform: &str,
    (first, unit): (UnitId, UnitId),
    scratch: &Path,
) -> Result<(String, bool), Fail> {
    // A first unit that runs during the test: its marker is in its argv and
    // in a file of its root. The test unit builds the marker from two parts,
    // so that the marker is in no argv of the test unit.
    let mark = format!("{}", std::process::id());
    let image = driver.prepare(payload, platform, false).map_err(|e| Fail::NoVerdict(format!("prepare: {e}")))?;
    let argv = vec!["sh".into(), "-c".into(), format!(": > podrunner-first-{mark}; sleep 60; :")];
    let spec = StartSpec {
        unit: first.clone(),
        image,
        argv,
        env: Default::default(),
        credential: Default::default(),
        sockets: None,
        mounts: Vec::new(),
        limits: Default::default(),
    };
    driver.start(&spec).map_err(|e| Fail::NoVerdict(format!("start the first unit: {e}")))?;
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
) -> Result<(String, bool), Fail> {
    let first_file = first_file(first_root).map_err(Fail::Verdict)?;
    let canary = scratch.join(format!("podrunner-guest-canary-{}", std::process::id()));
    std::fs::write(&canary, "canary").map_err(|e| Fail::NoVerdict(format!("the canary: {}", e.kind())))?;
    // A listener on the loopback of the host: a unit that reaches it shares
    // the network of the host. The unit tries with the tool it has. A host
    // that refuses the bind (#247) has no listener: the other lines still
    // hold (another kernel, no network interface), and the unit tries a
    // closed port.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").ok().filter(|l| l.set_nonblocking(true).is_ok());
    let port = listener.as_ref().and_then(|l| l.local_addr().ok()).map_or(1, |a| a.port());
    let me = std::process::id();
    let script = format!(
        "{{ echo boot_id=$(cat /proc/sys/kernel/random/boot_id); \
         echo kernel=$(cat /proc/version); \
         {net} \
         if [ -e '{canary}' ]; then echo canary=visible; else echo canary=absent; fi; \
         if [ -d /proc/{me} ] && grep -q podrunner /proc/{me}/cmdline 2>/dev/null; then echo hostpid=visible; else echo hostpid=absent; fi; \
         f=podrunner-first; f=$f-{mark}; p=absent; \
         for c in /proc/[0-9]*/cmdline; do case \"$(tr '\\0' ' ' < $c 2>/dev/null)\" in *\"$f\"*) p=visible;; esac; done; echo firstproc=$p; \
         if [ -e '{first_file}'/\"$f\" ]; then echo firstfile=visible; else echo firstfile=absent; fi; \
         }} > {RESULT}; \
         (command -v nc >/dev/null && nc -w 1 127.0.0.1 {port} </dev/null) >/dev/null 2>&1; \
         (command -v bash >/dev/null && bash -c 'exec 3<>/dev/tcp/127.0.0.1/{port}') >/dev/null 2>&1; true",
        canary = canary.display(),
        net = net_lines("/sys/class/net")
    );
    let result = run(driver, payload, platform, unit, &script, scratch);
    let _ = std::fs::remove_file(&canary);
    let reached = listener.is_some_and(|l| l.accept().is_ok());
    Ok((result?, reached))
}

/// The shell lines that list the network interfaces in `class`, other
/// than `lo`. The boundary is: the guest has no NIC that the machine gives
/// it, so no path out but the channel. `net=` names each interface with a
/// `device` link (a virtio-net or another NIC of the machine). A bridge, a
/// veth, a dummy or a tun made in the guest (the `docker0` of an engine)
/// has no device and, on a kernel of its own, no side on the host:
/// `net_virtual=` names it for the log, and it does not fail the test. A
/// `device` link that leads nowhere counts as a NIC: the test fails safe.
pub(crate) fn net_lines(class: &str) -> String {
    format!(
        "n=; v=; for i in $(ls '{class}' 2>/dev/null); do [ \"$i\" = lo ] && continue; \
         if [ -e '{class}'/\"$i\"/device ] || [ -L '{class}'/\"$i\"/device ]; \
         then n=\"${{n:+$n }}$i\"; else v=\"${{v:+$v }}$i\"; fi; done; \
         echo net=$n; echo net_virtual=$v;"
    )
}

/// The root of the first unit, in which the test unit looks for its file: a
/// driver that names none fails the test (#266), as the check of one unit
/// by another cannot run.
fn first_file(first_root: Option<&Path>) -> Result<String, String> {
    first_root
        .map(|r| r.display().to_string())
        .ok_or_else(|| "the driver names no root of the first unit: the check of one unit by another cannot run".into())
}

/// Start the test unit, wait for its end, and read its result file. A unit
/// with no result gives its exit code and the end of its log, read before
/// the destroy.
pub(crate) fn run(
    driver: &dyn UnitDriver,
    payload: &str,
    platform: &str,
    unit: UnitId,
    script: &str,
    scratch: &Path,
) -> Result<String, Fail> {
    let image = driver.prepare(payload, platform, false).map_err(|e| Fail::NoVerdict(format!("prepare: {e}")))?;
    let argv = vec!["sh".into(), "-c".into(), script.into()];
    let spec = StartSpec {
        unit: unit.clone(),
        image,
        argv,
        env: Default::default(),
        credential: Default::default(),
        sockets: None,
        mounts: Vec::new(),
        limits: Default::default(),
    };
    driver.start(&spec).map_err(|e| Fail::NoVerdict(format!("start: {e}")))?;
    let root = driver.observe(&unit).ok().and_then(|o| o.unit_root);
    let ended = driver.wait(&unit, Duration::from_secs(60));
    let text = root.and_then(|r| std::fs::read_to_string(r.join(RESULT)).ok());
    let got = match (ended, text) {
        (Err(e), _) => Err(format!("wait: {e}")),
        (Ok(None), _) => Err("the test unit did not end in 60 s".to_string()),
        (Ok(Some(_)), Some(t)) => Ok(t),
        (Ok(Some(exit)), None) => Err(format!("the test unit wrote no result the host can read ({})", exit_text(exit))),
    };
    let got = got.map_err(|why| Fail::NoVerdict(format!("{why}; {}", log_end(driver, &unit, scratch))));
    let _ = driver.destroy(&unit);
    got
}

fn exit_text(exit: Exit) -> String {
    match exit {
        Exit::Code(c) => format!("exit code {c}"),
        Exit::Signal(s) => format!("signal {s}"),
    }
}

/// The bounds of the read of the log of a test unit (T-570).
const LOG_READ_MAX: u64 = 8 << 20;
const LOG_READ_TIME: Duration = Duration::from_secs(5);

/// The most bytes of the end of the log in the text of an error: the error
/// is one line of the journal.
const LINE_MAX: usize = 1024;

/// The end of the log of the test unit, scrubbed, as one line. The destroy
/// removes the log of the unit: the scrubbed end (at most `LOG_TAIL_MAX`
/// bytes) stays in `SCRATCH/guest-test-KIND.log` for the operator.
fn log_end(driver: &dyn UnitDriver, unit: &UnitId, scratch: &Path) -> String {
    let log = match driver.unit_log(unit) {
        Ok(log) => log,
        Err(e) => return format!("no log of the unit: {e}"),
    };
    let mut scan = crate::log_tail::Scan::default();
    let read = log.read_bounded(LOG_READ_MAX, LOG_READ_TIME, &mut |b| scan.feed(b));
    scan.end();
    let tail = scan.tail(&crate::log_tail::Marks::default());
    let file = scratch.join(format!("guest-test-{}.log", driver.kind()));
    let kept = match std::fs::write(&file, &tail) {
        Ok(()) => format!(" (the end of the log is in {})", file.display()),
        Err(_) => String::new(),
    };
    if let Err(e) = read {
        return format!("the log of the unit: {e}{kept}");
    }
    let text = tail.trim_end();
    let mut cut = text.len().saturating_sub(LINE_MAX);
    while !text.is_char_boundary(cut) {
        cut += 1;
    }
    let end = text[cut..].trim_start().lines().collect::<Vec<_>>().join(" | ");
    if end.is_empty() {
        format!("the log of the unit is empty{kept}")
    } else {
        format!("the end of its log: {end}{kept}")
    }
}

/// What the host reads of its own kernel.
#[derive(Debug, Clone, Default)]
pub struct Host {
    pub boot_id: String,
    pub version: String,
}

/// The verdict on the lines of the test unit. Only an interface with a
/// device fails the line `net` (see [`net_lines`]). That rule holds only
/// for a kernel of its own, which the lines before it prove: on the kernel
/// of the host, a veth with no device can lead to the host.
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
        let virt = get("net_virtual");
        let also = if virt.is_empty() { String::new() } else { format!(" (not counted, with no device: {virt})") };
        return Err(format!("the unit has network interfaces with a device: {}{also}", get("net")));
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

/// The test runs again after this time, after a verdict.
pub const RETEST_MS: u64 = 24 * 3600 * 1000;

/// The first wait after a test with no verdict; each test in a row with
/// no verdict doubles it, up to [`BACKOFF_MAX_MS`].
pub const BACKOFF_MS: u64 = 2 * 60 * 1000;
pub const BACKOFF_MAX_MS: u64 = 30 * 60 * 1000;

/// The time from a test to the next, after `misses` tests in a row with no
/// verdict: [`RETEST_MS`] after a verdict.
pub fn next_ms(misses: u32) -> u64 {
    match misses {
        0 => RETEST_MS,
        n => (BACKOFF_MS << (n - 1).min(8)).min(BACKOFF_MAX_MS),
    }
}

/// The count of tests in a row with no verdict, after `verdict`.
pub fn misses_after(misses: u32, verdict: &Result<(), Fail>) -> u32 {
    if verdict.as_ref().is_err_and(Fail::no_verdict) { misses.saturating_add(1) } else { 0 }
}

/// The identity of a driver file: a new install changes it.
pub type Stamp = (u64, u64, i64);

/// The inode, the size and the time of change of `path`.
pub fn stamp(path: &Path) -> Option<Stamp> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| (m.ino(), m.size(), m.ctime()))
}

/// A kind that claims `guest`: its claim, and when slots tests it again
/// (each day, when its driver file changes, and in minutes after a test
/// with no verdict).
#[derive(Debug, Clone)]
pub struct Retest {
    pub claim: Capability,
    pub allowed: bool,
    pub payload: String,
    pub platform: String,
    pub last_ms: u64,
    pub driver: Option<std::path::PathBuf>,
    pub stamp: Option<Stamp>,
    /// The count of the last tests in a row with no verdict.
    pub misses: u32,
}

impl Retest {
    pub fn due(&self, now_ms: u64) -> bool {
        let changed = self.driver.as_deref().is_some_and(|d| stamp(d) != self.stamp);
        changed || now_ms.saturating_sub(self.last_ms) >= next_ms(self.misses)
    }

    /// Record a test at `now_ms`.
    pub fn tested(&mut self, now_ms: u64, verdict: &Result<(), Fail>) {
        self.last_ms = now_ms;
        self.stamp = self.driver.as_deref().and_then(stamp);
        self.misses = misses_after(self.misses, verdict);
    }
}

/// The unit ids of the self-test: above each id that a job gets.
pub fn test_ids() -> (UnitId, UnitId) {
    let id = |n: &str| UnitId::parse(n).expect("a unit id");
    (id("u-999999997"), id("u-999999998"))
}

/// The record of a kind after the test: a claim of `guest` that is not on
/// the allowlist, or that fails, drops to `none`.
pub fn settle(mut cap: Capability, allowed: bool, verdict: Result<(), Fail>) -> (Capability, Option<String>) {
    if cap.boundary != Boundary::Guest {
        return (cap, None);
    }
    let why = if !allowed {
        Some("the kind is not on guest_kinds".to_string())
    } else {
        verdict.err().map(|f| f.to_string())
    };
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
        // T-509: an interface with no device does not count.
        assert_eq!(judge(&good.replace("net=\n", "net=\nnet_virtual=docker0\n"), &host), Ok(()));
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

#[cfg(test)]
mod backoff_tests {
    use super::*;

    /// T-574: a test with no verdict waits minutes, doubled for each miss
    /// in a row and bounded; a verdict resets the count.
    #[test]
    fn a_test_with_no_verdict_backs_off_in_minutes() {
        let minutes: Vec<u64> = (1..=7).map(|n| next_ms(n) / 60_000).collect();
        assert_eq!(minutes, [2, 4, 8, 16, 30, 30, 30]);
        assert_eq!(next_ms(0), RETEST_MS);
        assert_eq!(next_ms(u32::MAX), BACKOFF_MAX_MS);
        let none = Err(Fail::NoVerdict("x".into()));
        assert_eq!(misses_after(2, &none), 3);
        assert_eq!(misses_after(2, &Err(Fail::Verdict("x".into()))), 0);
        assert_eq!(misses_after(2, &Ok(())), 0);
    }
}

#[cfg(test)]
mod first_tests {
    /// #266: a driver that names no root of the first unit fails the test.
    #[test]
    fn a_driver_with_no_root_of_the_first_unit_fails() {
        assert!(super::first_file(None).unwrap_err().contains("no root of the first unit"));
        assert_eq!(super::first_file(Some(std::path::Path::new("/u/1"))).unwrap(), "/u/1");
    }
}
