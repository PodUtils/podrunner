//! T-730: a signal and `destroy` reach each process of a guest, also one
//! that shows no tag. A process in its `exec` shows an empty environment;
//! these fake podvm processes drop the tag with `env -i` and keep their pid.

use podrunner_unit::conformance::alive;
use podrunner_unit::shim;

use super::*;
use crate::tests::{driver, spec};

const LIMIT: Duration = Duration::from_secs(10);

/// Wait for `event`; panic with `what` at the limit.
fn until(what: &str, mut event: impl FnMut() -> bool) {
    let end = Instant::now() + LIMIT;
    while !event() {
        assert!(Instant::now() < end, "{what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// `pid` runs `sleep` and shows no environment.
fn untagged_sleep(pid: u32) -> bool {
    fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|c| c.starts_with(b"sleep\0"))
        && fs::read(format!("/proc/{pid}/environ")).is_ok_and(|e| e.is_empty())
}

fn child_of(u: &MicrovmUnit, unit: &UnitId) -> u32 {
    u.units.lock().unwrap()[unit].child.id()
}

#[test]
fn a_term_just_after_start_reaches_a_podvm_that_shows_no_tag() {
    let (u, _d) = driver("term", "exec env -i sleep 300");
    let s = spec("u-1");
    u.start(&s).unwrap();
    let top = child_of(&u, &s.unit);
    until("podvm did not drop its tag", || untagged_sleep(top));
    assert!(procs::tagged(&procs::tag_of(&u.state_dir, "u-1")).is_empty(), "podvm still shows its tag");
    u.signal(&s.unit, Signal::Term).unwrap();
    let got = u.wait(&s.unit, LIMIT).unwrap();
    let residue = u.destroy(&s.unit).unwrap();
    assert_eq!(got, Some(Exit::Signal(libc::SIGTERM)), "the unit runs after TERM");
    assert!(residue.is_clean(), "{residue:?}");
}

#[test]
fn destroy_reaches_a_process_under_podvm_that_shows_no_tag() {
    let (u, _d) = driver("destroy", "env -i sleep 300 & wait");
    let s = spec("u-2");
    u.start(&s).unwrap();
    let top = child_of(&u, &s.unit);
    let mut below = None;
    until("no untagged process under podvm", || {
        below = shim::descendants(top).into_iter().find(|p| untagged_sleep(*p));
        below.is_some()
    });
    let below = below.unwrap();
    let residue = u.destroy(&s.unit).unwrap();
    let lives = alive(below);
    // The old code leaves it: end it here, so that the test leaves nothing.
    procs::kill(below, libc::SIGKILL);
    assert!(!lives, "process {below} lives after destroy");
    assert!(residue.is_clean(), "{residue:?}");
}
