//! Warm instances of the incus kind (T-487). A warm instance starts with
//! `podvm run -d` and no stdin: no credential, no job variable, no job
//! argv, and `PODRUNNER_WARM=1`. Its script marks the work directory ready
//! and waits. The claim is one `podvm exec -i`: its stdin carries the
//! credential files and the environment and the argv of the job (no word
//! of them in argv or the environment of podvm, no file of them on the
//! host). The script of the claim writes the files into the runner root at
//! mode 0600, `env.sh` into the 0700 directory `CLAIM_DIR`, then `go`.
//!
//! The limits are those of the kind: a job fits only when the lesser of
//! the kind and its offer gives the same (match or cold). podvm cannot
//! change the limits of a running instance yet (COORD-132, T-658).

use std::fs;
use std::io::Write;
use std::process::{Child, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use podrunner_schema::{Credential, UnitId};
use podrunner_unit::limits::Limits;
use podrunner_unit::warm::{ClaimSpec, WarmSpec, env_sh};
use podrunner_unit::{StartSpec, UnitCode, UnitError};

use crate::guest::credential_stream;
use crate::stderr::tail;
use crate::{IncusUnit, host};

/// The mark of a waiting instance, in its work directory.
pub const READY: &str = ".podrunner-warm";

/// The variable that makes the script of an instance wait for its claim.
pub const WARM_VAR: &str = "PODRUNNER_WARM";

/// The directory of the claim in the instance, mode 0700, owner root.
/// Both scripts name it as a word; a test holds them to it.
#[cfg(test)]
pub const CLAIM_DIR: &str = "/podrunner-claim";

/// The script of the claim, as root in the instance. `$1` is the runner
/// root. stdin holds the count N of the credential files, then N lines
/// `NAME BASE64`, then `env.sh`: the count frames the stream, so no name
/// can end the files early and put a line of its own into `env.sh`. A
/// name `.`, `..`, empty or with `/` ends the claim. `mkdir` of the claim
/// directory fails when it is there: one claim.
pub const CLAIM_SCRIPT: &str = r#"set -eu
root=$1
mkdir -p "$root"
umask 077
mkdir /podrunner-claim
read -r count
case "$count" in ''|*[!0-9]*) exit 97 ;; esac
i=0
while [ "$i" -lt "$count" ]; do
  read -r n b || exit 97
  case "$n" in ''|.|..|*/*) exit 97 ;; esac
  printf '%s' "$b" | base64 -d > "$root/$n"
  chmod 0600 "$root/$n"
  i=$((i + 1))
done
cat > /podrunner-claim/env.sh
: > /podrunner-claim/go.tmp
mv /podrunner-claim/go.tmp /podrunner-claim/go
"#;

/// The longest time of the `podvm exec` of a claim.
const CLAIM_LIMIT: Duration = Duration::from_secs(120);

/// The stdin of the claim: the count of the files, their lines, then
/// `script`. `credential_stream` refuses a name that is not plain.
pub fn claim_stream(credential: &Credential, script: &str) -> Result<Vec<u8>, UnitError> {
    let mut out = format!("{}\n", credential.files.len()).into_bytes();
    out.extend(credential_stream(credential)?);
    out.extend_from_slice(script.as_bytes());
    Ok(out)
}

/// The argv of the `podvm exec` of the claim of the instance `name`.
pub fn claim_args(name: &str, runner_root: &str) -> Vec<String> {
    ["exec", "-i", "--podvm-tier=incus", name, "/bin/sh", "-c", CLAIM_SCRIPT, "podrunner", runner_root]
        .map(String::from)
        .to_vec()
}

/// Wait up to `limit` for `child`; kill it after.
fn wait_bounded(child: &mut Child, limit: Duration) -> Result<ExitStatus, String> {
    let end = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return Ok(s),
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("no end in {} s", limit.as_secs()));
            }
            Err(e) => return Err(e.kind().to_string()),
        }
    }
}

impl IncusUnit {
    /// The limits that `start` gives an instance for an offer with `job`.
    pub(crate) fn applied(&self, job: &Limits) -> Limits {
        let mut l = self.limits.lesser(job);
        l.disk_bytes = l.disk_bytes.filter(|_| self.root_quota);
        l
    }

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
        let refuse = |why: String| UnitError::new(UnitCode::UnitStartFailed, why);
        // The instance started with the limits of the kind; no other.
        if self.applied(&spec.limits) != self.applied(&Limits::default()) {
            return Err(refuse("the limits of the offer are not those of the instance".into()));
        }
        let stream = claim_stream(&spec.credential, &env_sh(&spec.env, &spec.argv)?)?;
        let (name, dir) = {
            let mut units = self.units.lock().expect("the units");
            let r = units.get_mut(unit).ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, unit.to_string()))?;
            if r.claimed != Some(false) {
                return Err(UnitError::new(UnitCode::UnitInUse, format!("{unit} is not a free warm unit")));
            }
            // Claimed before the exec: a second claim is refused.
            r.claimed = Some(true);
            (r.name.clone(), r.dir.clone())
        };
        // The stderr of podvm goes to a file of the unit directory (0700):
        // a pipe that no one reads can stop podvm.
        let err_file = dir.join("claim.err");
        let err = fs::File::create(&err_file).map_err(|e| host("the claim", e))?;
        let mut child = self
            .podvm()
            .args(claim_args(&name, &self.cred_root))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(err)
            .spawn()
            .map_err(|e| refuse(format!("podvm exec: {}", e.kind())))?;
        // podvm reads all of stdin before it runs the command.
        let wrote = child.stdin.take().map(|mut i| i.write_all(&stream));
        let status = wait_bounded(&mut child, CLAIM_LIMIT).map_err(|why| refuse(format!("podvm exec: {why}")))?;
        let said = fs::read(&err_file).unwrap_or_default();
        let _ = fs::remove_file(&err_file);
        if !status.success() || !matches!(wrote, Some(Ok(()))) {
            return Err(refuse(format!("podvm exec of the claim: {}", tail(&said))));
        }
        Ok(())
    }
}
