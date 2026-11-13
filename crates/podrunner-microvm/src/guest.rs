//! The argv of `podvm run` for one guest, the script that the guest runs
//! first, and the private directory of the credential files.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use podrunner_schema::Credential;
use podrunner_unit::{UnitCode, UnitError};

/// Where the guest sees the credential directory (a 9p mount, read-only).
pub const CRED_MOUNT: &str = "/mnt/podrunner-cred";

/// Where the guest sees the work directory of the unit (a 9p mount): the
/// payload runs there, so a file of the job lands in the unit directory
/// of the host, and `destroy` removes it.
pub const WORK_MOUNT: &str = "/podrunner-work";

/// The script of the guest. `$1` is the runner root; the rest is the argv of
/// the payload. It copies the credential files into the runner root at
/// mode 0600, gives them to the user of the image (`runner`, when it
/// exists: the official runner refuses root), and runs the payload in the
/// work directory.
const SCRIPT: &str = r#"set -eu
root=$1; shift
mkdir -p "$root"
for f in /mnt/podrunner-cred/.[!.]* /mnt/podrunner-cred/*; do
  [ -f "$f" ] || continue
  n=${f##*/}
  cp "$f" "$root/$n"
  chmod 0600 "$root/$n"
done
cd /podrunner-work
if id runner >/dev/null 2>&1; then
  chown -R runner: "$root"
  exec setpriv --reuid=runner --regid=runner --init-groups -- "$@"
fi
exec "$@"
"#;

/// The argv of `podvm run` for one guest, with the credential directory
/// `UNIT_DIR/cred` and the work directory `UNIT_DIR/work`. The argv of the payload goes to
/// the script as positional parameters: no word of it is quoted again.
pub fn run_args(
    reference: &str,
    unit_dir: &Path,
    mem: &str,
    cpus: u32,
    runner_root: &str,
    env: &BTreeMap<String, String>,
    argv: &[String],
) -> Vec<String> {
    let mut a: Vec<String> =
        ["run", "--rm", "--podvm-tier=machine", "--podvm-mem", mem, "--podvm-cpus"].map(String::from).to_vec();
    a.push(cpus.to_string());
    a.push("-v".into());
    a.push(format!("{}:{CRED_MOUNT}:ro", unit_dir.join("cred").display()));
    a.push("-v".into());
    a.push(format!("{}:{WORK_MOUNT}", unit_dir.join("work").display()));
    for (k, v) in env {
        a.push("-e".into());
        a.push(format!("{k}={v}"));
    }
    a.push(reference.into());
    a.extend(["/bin/sh", "-c", SCRIPT, "podrunner", runner_root].map(String::from));
    a.extend(argv.iter().cloned());
    a
}

/// Make the work directory: mode 0777, so the user of the image writes in
/// it over 9p. Its parent, the unit directory, is 0700: no other user of
/// the host reaches it.
pub fn make_work(unit_dir: &Path, work: &Path) -> Result<(), UnitError> {
    let host = |e: std::io::Error| UnitError::new(UnitCode::Host, format!("the work directory: {}", e.kind()));
    fs::create_dir_all(work).map_err(host)?;
    fs::set_permissions(unit_dir, fs::Permissions::from_mode(0o700)).map_err(host)?;
    fs::set_permissions(work, fs::Permissions::from_mode(0o777)).map_err(host)
}

/// Write each credential file into `dir` (mode 0700) at mode 0600. A file
/// name that is not plain is refused.
pub fn write_credentials(dir: &Path, credential: &Credential) -> Result<(), UnitError> {
    let host = |what: &str, e: std::io::Error| UnitError::new(UnitCode::Host, format!("{what}: {}", e.kind()));
    fs::create_dir_all(dir).map_err(|e| host("the credential directory", e))?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|e| host("the credential directory", e))?;
    for f in &credential.files {
        if !f.name_is_plain() {
            return Err(UnitError::new(UnitCode::UnitStartFailed, "a credential file name that is not plain"));
        }
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join(&f.name))
            .map_err(|e| host("a credential file", e))?;
        out.write_all(f.content.expose().as_bytes()).map_err(|e| host("a credential file", e))?;
    }
    Ok(())
}
