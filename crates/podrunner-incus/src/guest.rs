//! The argv of `podvm run -d -i` for one instance, the script that the
//! instance runs first, and the stream of the credential files on stdin.

use std::collections::BTreeMap;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use podrunner_schema::Credential;
use podrunner_unit::{UnitCode, UnitError};

/// Where the instance sees the work directory of the unit (a `disk`
/// device): the payload runs there, so a file of the job lands in the unit
/// directory of the host, and `destroy` removes it.
pub const WORK_MOUNT: &str = "/podrunner-work";

/// The script of the instance, as root. `$1` is the runner root; the rest
/// is the argv of the payload. stdin holds one line for each credential
/// file, `NAME BASE64`: the script writes each file into the runner root at
/// mode 0600, gives the root to the user of the image (`runner`, when it
/// exists: the official runner refuses root), and runs the payload in the
/// work directory with no stdin. podvm removes the stdin file of the
/// instance once the script has it open.
pub const SCRIPT: &str = r#"set -eu
root=$1; shift
mkdir -p "$root"
while read -r n b; do
  case "$n" in ''|.|..|*/*) exit 97 ;; esac
  printf '%s' "$b" | base64 -d > "$root/$n"
  chmod 0600 "$root/$n"
done
cd /podrunner-work
if id runner >/dev/null 2>&1; then
  chown -R runner: "$root"
  exec setpriv --reuid=runner --regid=runner --init-groups -- "$@" </dev/null
fi
exec "$@" </dev/null
"#;

/// The argv of `podvm run -d -i` for the instance `name`, with the work
/// directory `work`. The argv of the payload goes to the script as
/// positional parameters: no word of it is quoted again.
pub fn run_args(
    name: &str,
    reference: &str,
    work: &Path,
    runner_root: &str,
    env: &BTreeMap<String, String>,
    argv: &[String],
) -> Vec<String> {
    let mut a: Vec<String> = ["run", "-d", "-i", "--name", name, "--podvm-tier=incus"].map(String::from).to_vec();
    a.push("-v".into());
    a.push(format!("{}:{WORK_MOUNT}", work.display()));
    for (k, v) in env {
        a.push("-e".into());
        a.push(format!("{k}={v}"));
    }
    a.push(reference.into());
    a.extend(["/bin/sh", "-c", SCRIPT, "podrunner", runner_root].map(String::from));
    a.extend(argv.iter().cloned());
    a
}

/// The stdin of the instance: one line `NAME BASE64` for each credential
/// file. A file name that is not plain is refused.
pub fn credential_stream(credential: &Credential) -> Result<Vec<u8>, UnitError> {
    let mut out = Vec::new();
    for f in &credential.files {
        if !f.name_is_plain() {
            return Err(UnitError::new(UnitCode::UnitStartFailed, "a credential file name that is not plain"));
        }
        out.extend_from_slice(f.name.as_bytes());
        out.push(b' ');
        out.extend_from_slice(STANDARD.encode(f.content.expose().as_bytes()).as_bytes());
        out.push(b'\n');
    }
    Ok(out)
}

/// The name of the instance of `unit`: unique for each state directory, so
/// two hosts on one incusd do not meet, and the same after a restart, so a
/// replay of the journal finds it. incus takes a letter first, then
/// letters, digits and `-`, at most 63.
pub fn instance_name(state_dir: &Path, unit: &str) -> String {
    // FNV-1a: stable over builds, unlike the hasher of the standard library.
    let mut h: u32 = 0x811c_9dc5;
    for b in state_dir.as_os_str().as_encoded_bytes() {
        h = (h ^ u32::from(*b)).wrapping_mul(0x0100_0193);
    }
    format!("podrunner-{h:08x}-{unit}")
}
