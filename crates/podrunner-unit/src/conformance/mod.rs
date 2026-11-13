//! The conformance suite of the unit seam: the tests 1 to 27 of
//! `docs/contracts/unit-driver.md`. A driver passes when each test that
//! applies to it passes. Each test has a planted defect that fails it (the
//! drivers of the fakes, GitHub #20) and a correct input that passes it.
//!
//! The tests run real processes and wait for events with a time limit;
//! they read `/proc` to see the processes of a unit.

mod cases;
mod design;
mod procs;

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use podrunner_schema::{Credential, CredentialFile, Secret, UnitId};

use crate::UnitDriver;

pub use design::ALONE;
pub use procs::{alive, holds_text};

/// The canary of the credential files: a made-up value that no output, argv
/// or environment may hold.
pub const CANARY: &str = "podrunner-canary-0f9c2d71";

/// The behaviour of a payload, which the fixture turns into argv.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behaviour {
    /// Exit with this code at once.
    Exit(i32),
    /// Run until a signal comes.
    Sleep,
    /// Start a child in a new session (`setsid`) that starts a grandchild;
    /// each runs until a signal comes.
    Grandchild,
    /// Write the file [`MARKER`] in the working directory, then sleep.
    Marker,
    /// Leave an orphan with no environment and a session of its own
    /// (`setsid env -i sleep`), then write its pid to [`ESCAPED`] in the
    /// working directory, then sleep.
    Escape,
    /// As [`Behaviour::Escape`], but kill the parent of the top process
    /// with SIGKILL (the shim of the driver, when it has one) after the pid
    /// file is written and the file [`GO`] exists: a parent with a death
    /// signal ends the shell with it. The test writes [`GO`] when `start`
    /// returns: a kill before it can end a launcher that has not yet told
    /// its driver that the payload started.
    KillParent,
    /// Copy each [`MARKER`] of the directories next to the working
    /// directory (`../*/MARKER`) to [`SEEN`], write and read back [`OWN`],
    /// then sleep.
    ReadOthers,
}

/// The file of [`Behaviour::KillParent`]: the test writes it in the
/// working directory when `start` returned.
pub const GO: &str = "podrunner-go";

/// The script of [`Behaviour::KillParent`] after the pid file: wait for
/// [`GO`], then kill the parent. A fixture puts it after its pid file.
pub const KILL_PARENT_AFTER_GO: &str = "while [ ! -e podrunner-go ]; do sleep 0.1; done; kill -9 $PPID; ";

/// The file of [`Behaviour::ReadOthers`]: what the payload read of the
/// roots next to its own.
pub const SEEN: &str = "podrunner-seen";

/// The file of [`Behaviour::ReadOthers`]: what the payload wrote and read
/// back in its own root (a positive control).
pub const OWN: &str = "podrunner-own";

/// The file that [`Behaviour::Escape`] writes.
pub const ESCAPED: &str = "podrunner-escaped";

/// The file that [`Behaviour::Marker`] writes.
pub const MARKER: &str = "podrunner-marker";

/// What a test needs from the driver under test.
pub trait Fixture {
    fn driver(&self) -> &dyn UnitDriver;
    /// A pinned reference that `prepare` accepts.
    fn image(&self) -> String;
    /// The platform of the host (`linux/amd64`).
    fn platform(&self) -> String;
    /// A platform that the host cannot run with no emulation.
    fn foreign_platform(&self) -> String;
    /// The argv of a payload with the behaviour.
    fn payload(&self, behaviour: Behaviour) -> Vec<String>;
    /// A fresh unit id.
    fn unit(&self) -> UnitId;
    /// The directory of the host where the driver keeps its state, when
    /// the fixture knows it: the scans of the tests 18 and 21 read it.
    fn state_dir(&self) -> Option<PathBuf> {
        None
    }
}

/// The result of one test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail(String),
    /// The test does not apply to this driver (for example, no container
    /// route). The reason is shown.
    Skip(String),
}

/// One test of the suite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub number: u32,
    pub name: &'static str,
    pub outcome: Outcome,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.outcome {
            Outcome::Pass => write!(f, "unit {:2} {}: pass", self.number, self.name),
            Outcome::Fail(why) => write!(f, "unit {:2} {}: FAIL: {why}", self.number, self.name),
            Outcome::Skip(why) => write!(f, "unit {:2} {}: skip: {why}", self.number, self.name),
        }
    }
}

fn all() -> impl Iterator<Item = &'static cases::Case> {
    cases::CASES.iter().chain(design::CASES.iter())
}

/// The number and the name of each test, in order.
pub fn tests() -> Vec<(u32, &'static str)> {
    all().map(|c| (c.number, c.name)).collect()
}

/// Run the tests with the numbers in `only` (all when empty), in order.
pub fn run(fixture: &dyn Fixture, only: &[u32]) -> Vec<Report> {
    all()
        .filter(|c| only.is_empty() || only.contains(&c.number))
        .map(|c| {
            let outcome = match (c.run)(fixture) {
                Ok(o) => o,
                Err(why) => Outcome::Fail(why),
            };
            Report { number: c.number, name: c.name, outcome }
        })
        .collect()
}

/// Run `f` on a thread of its own and wait up to `limit` for its value.
/// `None`: it did not end; the thread stays, and the end of the process
/// ends it.
pub fn bounded<T: Send + 'static>(limit: Duration, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(limit).ok()
}

/// Run the test `number` with a time limit: a test that hangs fails with
/// its name, and the suite goes on (T-099).
pub fn run_bounded(fixture: Arc<dyn Fixture + Send + Sync>, number: u32, limit: Duration) -> Option<Report> {
    let case = all().find(|c| c.number == number)?;
    let (name, run) = (case.name, case.run);
    let outcome = bounded(limit, move || match run(&*fixture) {
        Ok(o) => o,
        Err(why) => Outcome::Fail(why),
    })
    .unwrap_or_else(|| Outcome::Fail(format!("no end in {} s: the test hangs", limit.as_secs())));
    Some(Report { number, name, outcome })
}

/// The numbers of the tests that did not pass (a skip is not a pass).
pub fn not_passed(results: &[Report]) -> Vec<u32> {
    results.iter().filter(|r| r.outcome != Outcome::Pass).map(|r| r.number).collect()
}

/// The credential of the tests: two files that hold [`CANARY`].
pub fn canary_credential() -> Credential {
    Credential {
        files: vec![
            CredentialFile { name: ".runner".into(), content: Secret::new(format!("{{\"agentName\":\"{CANARY}\"}}")) },
            CredentialFile { name: ".credentials".into(), content: Secret::new(format!("{{\"token\":\"{CANARY}\"}}")) },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_gives_the_value_or_none_at_the_limit() {
        assert_eq!(bounded(Duration::from_secs(5), || 7), Some(7));
        let t = std::time::Instant::now();
        assert_eq!(bounded(Duration::from_millis(200), || std::thread::sleep(Duration::from_secs(30))), None);
        assert!(t.elapsed() < Duration::from_secs(5));
    }
}
