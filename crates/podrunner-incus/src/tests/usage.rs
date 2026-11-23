//! T-483: the incus kind keeps the use that incusd gave while the instance
//! ran. The instance stops when its command ends, and incusd gives no use
//! of a stopped instance: a read at `usage` time alone gives `-`.

use std::time::Instant;

use podrunner_unit::UnitUsage;

use super::*;

/// The payload holds until the test makes `B.end`, then exits 3.
const HOLD: &str = r#"while [ ! -f "$B.end" ]; do sleep 0.05; done; exit 3"#;

/// The prefix of the files of the instance of `unit` in the fake store.
fn files(u: &IncusUnit, d: &Path, unit: &UnitId) -> String {
    d.join("store").join(u.name(unit)).display().to_string()
}

/// Wait for `cond`, at most 10 s.
fn until(what: &str, cond: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !cond() {
        assert!(Instant::now() < end, "no {what} in 10 s");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// End the payload of `HOLD`, and wait until the fake records its stop.
fn end_payload(prefix: &str) {
    fs::write(format!("{prefix}.end"), "").unwrap();
    until("stop", || fs::read_to_string(format!("{prefix}.status")).is_ok_and(|s| s.starts_with("Stopped")));
}

#[test]
fn the_use_is_the_last_sample_while_the_instance_ran() {
    let (u, d) = driver("use-last", HOLD);
    let s = spec("u-1");
    u.start(&s).unwrap();
    let f = files(&u, &d, &s.unit);
    // Each read of the state while the payload runs is a sample.
    for sample in ["- -", "1.5 5000", "2.25 3000"] {
        fs::write(format!("{f}.use"), sample).unwrap();
        assert_eq!(u.wait(&s.unit, Duration::ZERO).unwrap(), None);
    }
    end_payload(&f);
    assert_eq!(u.wait(&s.unit, Duration::from_secs(10)).unwrap(), Some(Exit::Code(3)));
    // The stopped instance gives `- -`: the kept use stays. The peak is the
    // highest of the samples.
    let want = UnitUsage { cpu_s: Ok(3), peak_memory_bytes: Ok(5000) };
    assert_eq!(u.usage(&s.unit), Some(want.clone()));
    assert_eq!(u.usage(&s.unit), Some(want), "a second read keeps it");
    assert!(u.destroy(&s.unit).unwrap().is_clean());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_dash_while_the_instance_ran_keeps_the_reason_of_incusd() {
    let (u, d) = driver("use-dash", HOLD);
    let s = spec("u-2");
    u.start(&s).unwrap();
    let f = files(&u, &d, &s.unit);
    fs::write(format!("{f}.use"), "- 7000").unwrap();
    assert_eq!(u.wait(&s.unit, Duration::ZERO).unwrap(), None);
    end_payload(&f);
    assert_eq!(u.wait(&s.unit, Duration::from_secs(10)).unwrap(), Some(Exit::Code(3)));
    let got = u.usage(&s.unit).unwrap();
    assert_eq!(got.cpu_s, Err("incusd gave no CpuSeconds of the instance".into()));
    assert_eq!(got.peak_memory_bytes, Ok(7000));
    assert!(u.destroy(&s.unit).unwrap().is_clean());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn an_instance_that_ended_before_a_sample_gives_the_reason() {
    let (u, d) = driver("use-none", "exit 0");
    let s = spec("u-3");
    u.start(&s).unwrap();
    let f = files(&u, &d, &s.unit);
    until("stop", || fs::read_to_string(format!("{f}.status")).is_ok_and(|s| s.starts_with("Stopped")));
    assert_eq!(u.wait(&s.unit, Duration::from_secs(10)).unwrap(), Some(Exit::Code(0)));
    let got = u.usage(&s.unit).unwrap();
    for e in [got.cpu_s.unwrap_err(), got.peak_memory_bytes.unwrap_err()] {
        assert!(e.contains("ended before a read of its use"), "{e}");
    }
    assert!(u.destroy(&s.unit).unwrap().is_clean());
    let _ = fs::remove_dir_all(&d);
}

/// A podvm before `0.2.0-beta.33` refuses the fields of the use: the read
/// of the state falls back to the status, so the unit still ends.
#[test]
fn a_podvm_with_no_fields_of_the_use_still_ends_the_unit() {
    let (u, d) = driver("use-old", HOLD);
    fs::write(d.join("podvm.old"), "").unwrap();
    let s = spec("u-4");
    u.start(&s).unwrap();
    let f = files(&u, &d, &s.unit);
    assert_eq!(u.wait(&s.unit, Duration::ZERO).unwrap(), None, "the instance runs");
    end_payload(&f);
    assert_eq!(u.wait(&s.unit, Duration::from_secs(10)).unwrap(), Some(Exit::Code(3)));
    let got = u.usage(&s.unit).unwrap();
    assert!(got.cpu_s.as_ref().is_err_and(|e| e.contains("0.2.0-beta.33")), "{got:?}");
    assert!(u.destroy(&s.unit).unwrap().is_clean());
    let _ = fs::remove_dir_all(&d);
}
