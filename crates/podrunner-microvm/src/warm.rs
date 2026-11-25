//! Warm guests of the microvm kind (T-487). A warm guest boots with an
//! empty credential directory and `PODRUNNER_WARM=1`; its script marks the
//! work directory ready and waits for `.podrunner-claim/go` in the
//! read-only credential mount (9p shows a file that the host writes after
//! the boot). The claim writes the credential files, then `env.sh` (the
//! environment and the argv of the job), then `go`. The script reads them
//! BEFORE it starts the engine, so `dockerd` gets the variables of the job
//! (the CA of its broker). The limits are those of the kind: a job fits
//! only when the lesser of the kind and its offer gives the same.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use podrunner_schema::{Credential, UnitId};
use podrunner_unit::limits::Limits;
use podrunner_unit::warm::{ClaimSpec, WarmSpec, env_sh};
use podrunner_unit::{StartSpec, UnitCode, UnitError};

use crate::MicrovmUnit;

/// The mark of a waiting guest, in its work directory.
pub const READY: &str = ".podrunner-warm";

/// The directory of the claim, in the credential directory.
pub const CLAIM: &str = ".podrunner-claim";

/// The variable that makes the script of a guest wait for its claim.
pub const WARM_VAR: &str = "PODRUNNER_WARM";

fn host(what: &str, e: std::io::Error) -> UnitError {
    UnitError::new(UnitCode::Host, format!("{what}: {}", e.kind()))
}

/// Write the claim into the credential directory `cred`: the files at mode
/// 0600, `CLAIM/env.sh` at 0600, then `CLAIM/go` by a rename.
pub(crate) fn write_claim(cred: &Path, credential: &Credential, script: &str) -> Result<(), UnitError> {
    crate::guest::write_credentials(cred, credential)?;
    let dir = cred.join(CLAIM);
    fs::create_dir_all(&dir).map_err(|e| host("the claim", e))?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|e| host("the claim", e))?;
    let write = |name: &str, bytes: &[u8]| -> Result<(), UnitError> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join(name))
            .map_err(|e| host("the claim", e))?;
        f.write_all(bytes).map_err(|e| host("the claim", e))
    };
    write("env.sh", script.as_bytes())?;
    write("go.tmp", b"")?;
    fs::rename(dir.join("go.tmp"), dir.join("go")).map_err(|e| host("the claim", e))
}

impl MicrovmUnit {
    pub(crate) fn warm_unit(&self, spec: &WarmSpec) -> Result<(), UnitError> {
        let mut env = spec.env.clone();
        env.insert(WARM_VAR.into(), "1".into());
        let start = StartSpec {
            unit: spec.unit.clone(),
            image: spec.image.clone(),
            argv: Vec::new(),
            env,
            credential: Credential { files: Vec::new() },
            sockets: spec.sockets.clone(),
            mounts: Vec::new(),
            limits: Limits::default(),
        };
        self.launch(&start, Some(false))
    }

    pub(crate) fn warm_ready_of(&self, unit: &UnitId) -> Result<bool, UnitError> {
        let units = self.units.lock().expect("the units");
        let r = units.get(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
        Ok(fs::symlink_metadata(r.dir.join("work").join(READY)).is_ok_and(|m| m.is_file()))
    }

    pub(crate) fn claim_unit(&self, unit: &UnitId, spec: &ClaimSpec) -> Result<(), UnitError> {
        // The guest booted with the limits of the kind; no other (P2-5).
        if self.applied(&spec.limits) != self.applied(&Limits::default()) {
            return Err(UnitError::new(
                UnitCode::UnitStartFailed,
                "the limits of the offer are not those of the guest",
            ));
        }
        let script = env_sh(&spec.env, &spec.argv)?;
        let dir = {
            let mut units = self.units.lock().expect("the units");
            let r = units.get_mut(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
            if r.claimed != Some(false) {
                return Err(UnitError::new(UnitCode::UnitInUse, format!("{unit} is not a free warm unit")));
            }
            // Claimed before the first write: a second claim is refused.
            r.claimed = Some(true);
            r.dir.clone()
        };
        write_claim(&dir.join("cred"), &spec.credential, &script)
    }
}
