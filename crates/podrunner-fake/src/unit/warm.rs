//! Warm units of the fake kind (T-487): the top process waits in the root
//! of its unit until the claim writes `.podrunner-claim/go`, then reads the
//! environment and the argv of the job and runs them.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use podrunner_schema::UnitId;
use podrunner_unit::procs;
use podrunner_unit::warm::{ClaimSpec, WarmSpec, env_sh};
use podrunner_unit::{StartSpec, UnitCode, UnitError};

use super::{FakeUnit, UnitPlant, host};

/// The mark of a waiting unit, in its root.
const READY: &str = ".podrunner-warm";

/// The directory of the claim, in the root of the unit.
const CLAIM: &str = ".podrunner-claim";

/// The top process of a warm unit: it waits for `go`, then runs the claim.
const WAITER: &str = "touch .podrunner-warm; while [ ! -e .podrunner-claim/go ]; do sleep 0.05; done; \
                      rm -f .podrunner-warm; . ./.podrunner-claim/env.sh; rm -rf .podrunner-claim; exec \"$@\"";

impl FakeUnit {
    pub(super) fn warm_unit(&self, spec: &WarmSpec) -> Result<(), UnitError> {
        let mut units = self.units.lock().expect("the units");
        if units.contains_key(&spec.unit) && !self.planted(UnitPlant::StartTwice) {
            return Err(UnitError::new(UnitCode::UnitInUse, spec.unit.to_string()));
        }
        let (root, runner_root, _) = self.make_root(&spec.unit)?;
        if self.planted(UnitPlant::WarmCredential)
            && let Some(c) = self.last_claim.lock().expect("the last claim").clone()
        {
            self.write_credential(&runner_root, &c)?;
        }
        let argv = ["sh", "-c", WAITER, "podrunner"].map(String::from);
        let env = spec.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let mut run = self.launch(&spec.unit, root, runner_root, &argv, env, spec.sockets.clone())?;
        run.claimed = Some(false);
        units.insert(spec.unit.clone(), run);
        Ok(())
    }

    pub(super) fn warm_ready_of(&self, unit: &UnitId) -> Result<bool, UnitError> {
        self.with_unit(unit, |r| Ok(fs::symlink_metadata(r.root.join(READY)).is_ok_and(|m| m.is_file())))
    }

    pub(super) fn claim_unit(&self, unit: &UnitId, spec: &ClaimSpec) -> Result<(), UnitError> {
        let twice = self.planted(UnitPlant::ClaimTwice);
        let (root, runner_root) = self.with_unit(unit, |r| match r.claimed {
            Some(false) => {
                // Claimed before the first write: a second claim is refused.
                r.claimed = Some(true);
                Ok((r.root.clone(), r.runner_root.clone()))
            }
            Some(true) if twice => Ok((r.root.clone(), r.runner_root.clone())),
            _ => Err(UnitError::new(UnitCode::UnitInUse, format!("{unit} is not a free warm unit"))),
        })?;
        if twice && root.join(CLAIM).join("go").exists() {
            return Ok(());
        }
        let mut env = spec.env.clone();
        if self.planted(UnitPlant::CredentialInEnv)
            && let Some(f) = spec.credential.files.first()
        {
            env.insert("PODRUNNER_PLANTED_CREDENTIAL".into(), f.content.expose().to_string());
        }
        let argv = if self.planted(UnitPlant::ClaimBootArgv) { vec!["true".to_string()] } else { spec.argv.clone() };
        let script = env_sh(&env, &argv)?;
        self.write_credential(&runner_root, &spec.credential)?;
        *self.last_claim.lock().expect("the last claim") = Some(spec.credential.clone());
        let start = StartSpec {
            unit: unit.clone(),
            image: podrunner_unit::Prepared { digest: String::new(), platform: String::new() },
            argv: argv.clone(),
            env: BTreeMap::new(),
            credential: spec.credential.clone(),
            sockets: None,
            mounts: Vec::new(),
            limits: spec.limits,
        };
        self.plant_start(&start, &root)?;
        let dir = root.join(CLAIM);
        fs::create_dir_all(&dir).map_err(|e| host(e, "the claim"))?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|e| host(e, "the claim"))?;
        super::write_private(&dir.join("env.sh"), script.as_bytes(), 0o600)?;
        // `go` comes last, by a rename: the waiter sees a whole claim.
        super::write_private(&dir.join("go.tmp"), b"", 0o600)?;
        fs::rename(dir.join("go.tmp"), dir.join("go")).map_err(|e| host(e, "the claim"))?;
        // The claim ends when the waiter runs the job: a
        // read of the unit then sees the job, not the waiter and its `sleep`
        // and `rm` (a false pass of test 7 with the plant `CredentialInEnv`).
        let tag = self.with_unit(unit, |r| Ok(r.tag.clone()))?;
        let end = Instant::now() + CLAIM_LIMIT;
        while Instant::now() < end && procs::tagged(&tag).into_iter().any(waits) {
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}

/// The time that a claim waits for the waiter to run the job.
const CLAIM_LIMIT: Duration = Duration::from_secs(10);

/// True when the process is the waiter before its `exec`.
fn waits(pid: u32) -> bool {
    fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|c| c.windows(GO.len()).any(|w| w == GO.as_bytes()))
}

/// The file of the claim that the argv of the waiter names.
const GO: &str = ".podrunner-claim/go";
