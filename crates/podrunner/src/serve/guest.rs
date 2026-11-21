//! The claim of `guest` of a unit kind at the start of `serve`. The claim
//! holds only on the allowlist and after the self-test of GitHub #36;
//! slots tests it again each day, on a change of the driver file, and in
//! minutes after a test with no verdict (T-574).

use std::path::{Path, PathBuf};

use podrunner_schema::Capability;
use podrunner_slots::config::{Config, UnitConfig};
use podrunner_slots::guest::{self, Fail, Retest};
use podrunner_unit::UnitDriver;

/// The record of kind `name` after the self-test, and its retest when a
/// payload is pinned for its arch.
pub(super) fn claim(
    name: &str,
    u: &UnitConfig,
    cfg: &Config,
    driver: &dyn UnitDriver,
    capability: Capability,
    state: &Path,
) -> (Capability, Option<Retest>) {
    let allowed = cfg.guest_kinds.iter().any(|k| k == name);
    let pin = cfg.pin.first_for(&capability.arch, u.init);
    let platform = if capability.arch == "aarch64" { "linux/arm64" } else { "linux/amd64" };
    let verdict = match (allowed, pin) {
        (true, Some(p)) => guest::self_test(driver, &p.reference, platform, guest::test_ids(), state),
        (true, None) => Err(Fail::Verdict("no pinned payload for the self-test".into())),
        (false, _) => Err(Fail::Verdict("not on guest_kinds".into())),
    };
    let misses = guest::misses_after(0, &verdict);
    let claim = capability.clone();
    let (cap, note) = guest::settle(capability, allowed, verdict);
    if let Some(why) = note {
        eprintln!("podrunner: [unit.{name}]: the claim of guest drops to none: {why}");
    }
    let retest = pin.map(|p| {
        let driver = u.binary.as_ref().map(PathBuf::from);
        let stamp = driver.as_deref().and_then(guest::stamp);
        Retest {
            claim,
            allowed,
            payload: p.reference.clone(),
            platform: platform.into(),
            // The time of the slots starts after this test.
            last_ms: 0,
            driver,
            stamp,
            misses,
        }
    });
    (cap, retest)
}

#[cfg(test)]
mod tests {
    use podrunner_schema::Boundary;
    use podrunner_slots::guest::{BACKOFF_MS, RETEST_MS};

    const PIN: &str = "runner@sha256:1111111111111111111111111111111111111111111111111111111111111111";

    fn cfg() -> podrunner_slots::config::Config {
        let arch = std::env::consts::ARCH;
        podrunner_slots::config::Config::parse(&format!(
            "host = \"h\"\nallow_fake = true\nguest_kinds = [\"fake\"]\n[unit.fake]\nslots = 1\nclaim = \"guest\"\n\
             [pin]\n[[pin.payload]]\narch = \"{arch}\"\nreference = \"{PIN}\"\nbuilt_at_s = 1800000000\n"
        ))
        .unwrap()
    }

    /// A self-test with no verdict (a driver that starts no unit) takes the
    /// kind out for minutes, not for a day; a failed verdict (a fake on the
    /// kernel of the host) keeps the day.
    #[test]
    fn the_retest_of_a_kind_with_no_verdict_is_due_in_minutes() {
        let cfg = cfg();
        let u = &cfg.unit["fake"];
        let claim = crate::serve::fake::fake_capability_of(u);
        let dir = std::env::temp_dir().join(format!("podrunner-serve-guest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (cap, retest) = super::claim("fake", u, &cfg, &podrunner_unit::Stub, claim.clone(), &dir);
        assert_eq!(cap.boundary, Boundary::None);
        let r = retest.expect("a retest");
        assert!(!r.due(BACKOFF_MS - 1));
        assert!(r.due(BACKOFF_MS));
        let platform = if claim.arch == "aarch64" { "linux/arm64" } else { "linux/amd64" };
        let fake = podrunner_fake::FakeUnit::new(&dir.join("fake"), claim.clone(), platform);
        let (cap, retest) = super::claim("fake", u, &cfg, &fake, claim, &dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(cap.boundary, Boundary::None);
        let r = retest.expect("a retest");
        assert!(!r.due(RETEST_MS - 1));
        assert!(r.due(RETEST_MS));
    }
}
