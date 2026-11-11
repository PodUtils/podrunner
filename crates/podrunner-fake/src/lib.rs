//! The fakes of podrunner (GitHub #20): the `fake` source and the `fake`
//! unit. Level 1 runs on them, offline, with a manual clock. Each fake can
//! carry one planted defect, which fails one conformance test: the fakes
//! are the correct inputs and the plants of the harnesses of
//! `podrunner-unit` and `podrunner-source`. `docs/fakes.md` gives the
//! formats.

mod mounts;
mod source;
mod unit;

pub use podrunner_unit::procs::{TAG_VAR, tag_of, tagged};
pub use source::{FakeSource, SourcePlant, Step, What, canned_credential, parse_script};
pub use unit::{FakeUnit, UnitPlant};

use podrunner_unit::conformance::Behaviour;

/// The argv of a payload of the fake unit with the behaviour (a POSIX shell
/// and `setsid` on the host). Each process ends after 300 s at most.
pub fn payload(behaviour: Behaviour) -> Vec<String> {
    let sh = |s: &str| vec!["sh".to_string(), "-c".to_string(), s.to_string()];
    match behaviour {
        Behaviour::Exit(n) => sh(&format!("exit {n}")),
        Behaviour::Sleep => vec!["sleep".into(), "300".into()],
        Behaviour::Grandchild => sh("setsid sh -c 'sleep 300 & wait' & wait"),
        Behaviour::Marker => sh(&format!("echo x > {}; exec sleep 300", podrunner_unit::conformance::MARKER)),
    }
}

#[cfg(test)]
mod tests;
