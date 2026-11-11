//! The guest floor self-test (GitHub #36). `untrusted` jobs need boundary
//! `guest`, a separate kernel, and a driver only declares its boundary. So
//! slots accepts `guest` only from a kind on the allowlist (`guest_kinds`,
//! default `microvm`), after a self-test unit proves it; else the claim
//! drops to `none`.
//!
//! The test unit writes what it sees into its working directory: the boot
//! id of its kernel, its network interfaces other than `lo`, whether a
//! canary file that slots planted on the host is visible, and whether the
//! process of slots is visible. The host and the unit share no boot id.

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
    unit: UnitId,
    scratch: &Path,
) -> Result<(), String> {
    let canary = scratch.join(format!("podrunner-guest-canary-{}", std::process::id()));
    std::fs::write(&canary, "canary").map_err(|e| format!("the canary: {}", e.kind()))?;
    let me = std::process::id();
    let script = format!(
        "{{ echo boot_id=$(cat /proc/sys/kernel/random/boot_id); \
         echo net=$(ls /sys/class/net 2>/dev/null | grep -vx lo | tr '\\n' ' '); \
         if [ -e '{canary}' ]; then echo canary=visible; else echo canary=absent; fi; \
         if [ -d /proc/{me} ] && grep -q podrunner /proc/{me}/cmdline 2>/dev/null; then echo hostpid=visible; else echo hostpid=absent; fi; \
         }} > {RESULT}",
        canary = canary.display()
    );
    let result = run(driver, payload, platform, unit, &script);
    let _ = std::fs::remove_file(&canary);
    let text = result?;
    let host_boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap_or_default();
    judge(&text, host_boot.trim())
}

/// Start the test unit, wait for its end, and read its result file.
fn run(driver: &dyn UnitDriver, payload: &str, platform: &str, unit: UnitId, script: &str) -> Result<String, String> {
    let image = driver.prepare(payload, platform, false).map_err(|e| format!("prepare: {e}"))?;
    let argv = vec!["sh".into(), "-c".into(), script.into()];
    let spec = StartSpec { unit: unit.clone(), image, argv, env: Default::default(), credential: Default::default() };
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

/// The verdict on the lines of the test unit.
pub fn judge(text: &str, host_boot_id: &str) -> Result<(), String> {
    let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).map(str::trim).unwrap_or("");
    let boot = get("boot_id");
    if boot.is_empty() || boot == host_boot_id {
        return Err("the unit runs on the kernel of the host (the same boot id)".into());
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
    Ok(())
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
        let good = "boot_id=b2\nnet=\ncanary=absent\nhostpid=absent\n";
        assert_eq!(judge(good, "b1"), Ok(()));
        for (bad, why) in [
            ("boot_id=b1\nnet=\ncanary=absent\nhostpid=absent", "same boot id"),
            ("boot_id=b2\nnet=eth0\ncanary=absent\nhostpid=absent", "network"),
            ("boot_id=b2\nnet=\ncanary=visible\nhostpid=absent", "file of the host"),
            ("boot_id=b2\nnet=\ncanary=absent\nhostpid=visible", "process of the host"),
            ("", "same boot id"),
        ] {
            assert!(judge(bad, "b1").unwrap_err().contains(why), "{bad}");
        }
    }
}
