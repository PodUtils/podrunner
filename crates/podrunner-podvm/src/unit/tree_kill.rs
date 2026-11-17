//! The proof of the tree kill of a podvm unit on this host (unit test 3),
//! and the reference of an image that `podvm load` gave.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use podrunner_schema::UnitId;
use podrunner_unit::procs;
use podrunner_unit::{StartSpec, UnitDriver, UnitError};

use super::PodvmUnit;

/// True when the process is podvm itself (its name).
pub(super) fn procs_comm_is_podvm(pid: u32) -> bool {
    procs_comm_is(pid, "podvm")
}

fn procs_comm_is(pid: u32, name: &str) -> bool {
    fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|c| c.trim().starts_with(name))
}

impl PodvmUnit {
    /// Prove the tree kill on this host with the pinned image `reference`
    /// (unit test 3): a payload with a `setsid` grandchild, then destroy.
    /// When no process is left, the record says `tree_kill: verified`, and
    /// a `wall` limit can be enforced. The image needs `sh`, `setsid` and
    /// `sleep`.
    pub fn verify_tree_kill(&mut self, reference: &str, unit: UnitId) -> Result<bool, UnitError> {
        let image = self.prepare(reference, &self.platform.clone(), false)?;
        // A unit of the last self-test, of a process that ended (T-183).
        let _ = self.destroy(&unit);
        let held = self.self_test_sockets(&unit)?;
        let argv = ["sh", "-c", "setsid sh -c 'sleep 300 & wait' & wait"].map(String::from).to_vec();
        let spec = StartSpec {
            unit: unit.clone(),
            image,
            argv,
            env: Default::default(),
            credential: Default::default(),
            sockets: held.as_ref().map(|h| h.dir.clone()),
            mounts: Vec::new(),
            limits: Default::default(),
        };
        self.start(&spec)?;
        let tag = procs::tag_of(&self.state_dir, unit.as_str());
        let end = Instant::now() + Duration::from_secs(10);
        let mut seen = Vec::new();
        while Instant::now() < end {
            seen = procs::tagged(&tag);
            if seen.iter().filter(|p| !procs_comm_is_podvm(**p)).count() >= 3 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let residue = self.destroy(&unit)?;
        let verified =
            seen.len() >= 3 && residue.is_clean() && seen.iter().all(|p| !podrunner_unit::conformance::alive(*p));
        if verified {
            self.capability.tree_kill = podrunner_schema::TreeKill::Verified;
            self.capability.limits.insert("wall".into(), podrunner_schema::LimitMode::Enforced);
        }
        Ok(verified)
    }

    /// The reference of the one image of a unit store: `podvm images`
    /// names the repository and the tag that `load` gave it.
    pub(super) fn image_reference(&self, store: &Path) -> Option<String> {
        let out = self.podvm(store, &["images"], None).ok()?;
        let line = out.lines().nth(1)?;
        let mut cols = line.split_whitespace();
        Some(format!("{}:{}", cols.next()?, cols.next()?))
    }
}
