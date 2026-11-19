//! The argv of `podvm run -d -i` for one instance, the script that the
//! instance runs first, and the stream of the credential files on stdin.

use std::collections::BTreeMap;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use podrunner_schema::{Credential, PlainName};
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
# The channel to the egress socket: the proxy of the payload.
if [ -n "${PODRUNNER_CHANNEL_PORT:-}" ]; then
  u=http://127.0.0.1:$PODRUNNER_CHANNEL_PORT
  export http_proxy=$u https_proxy=$u HTTP_PROXY=$u HTTPS_PROXY=$u
  # The fetch of Node takes the proxy only with this (#272).
  export NODE_USE_ENV_PROXY="${NODE_USE_ENV_PROXY:-1}"
fi
mkdir -p "$root"
while read -r n b; do
  case "$n" in ''|.|..|*/*) exit 97 ;; esac
  printf '%s' "$b" | base64 -d > "$root/$n"
  chmod 0600 "$root/$n"
done
# The engine of the unit (T-485): dockerd as root in the instance, from
# the read-only engine directory, its socket in the instance only. The
# instance has no NIC: no bridge and no iptables; the pulls go through the
# proxy of the channel.
if [ -x /podrunner-engine/dockerd ]; then
  PATH=/podrunner-engine:$PATH dockerd --host=unix:///run/podrunner-docker.sock --iptables=false --ip6tables=false \
    --bridge=none >/tmp/podrunner-dockerd.log 2>&1 &
  i=0
  while [ ! -S /run/podrunner-docker.sock ] && [ $i -lt 60 ]; do sleep 1; i=$((i + 1)); done
  if [ ! -S /run/podrunner-docker.sock ]; then
    echo "podrunner: the engine did not start" >&2
    tail -n 20 /tmp/podrunner-dockerd.log >&2
    exit 95
  fi
  if id runner >/dev/null 2>&1; then chown runner /run/podrunner-docker.sock; fi
  export DOCKER_HOST=unix:///run/podrunner-docker.sock PATH=/podrunner-engine:$PATH
fi
cd /podrunner-work
if id runner >/dev/null 2>&1; then
  chown -R runner: "$root"
  # setpriv keeps the environment of root: the home of the user (T-178).
  home=$(getent passwd runner | cut -d: -f6)
  export HOME="${home:-/home/runner}" USER=runner LOGNAME=runner
  exec setpriv --reuid=runner --regid=runner --init-groups -- "$@" </dev/null
fi
exec "$@" </dev/null
"#;

/// The smallest root disk that podvm gives an instance.
pub const ROOT_MIN: u64 = 64 << 20;

/// The port in the instance of the channel to the egress socket.
pub const EGRESS_PORT: u16 = 3128;

/// The variable that gives the script the port of the egress channel; the
/// script makes the proxy variables of the payload from it.
pub const CHANNEL_PORT_VAR: &str = "PODRUNNER_CHANNEL_PORT";

/// The fixed parts of one instance.
#[derive(Debug, Clone, Copy)]
pub struct Instance<'a> {
    pub name: &'a str,
    pub reference: &'a str,
    pub work: &'a Path,
    pub egress: Option<&'a Path>,
    pub runner_root: &'a str,
    pub limits: &'a podrunner_unit::limits::Limits,
    /// The directory of the static Docker binaries: each instance runs an
    /// engine of its own (T-485).
    pub engine: Option<&'a Path>,
    /// The init of the image is PID 1 (systemd of the hosted snapshot),
    /// with its own services and engine (T-543).
    pub init_image: bool,
}

/// Where the instance sees the engine directory, read-only.
pub const ENGINE_MOUNT: &str = "/podrunner-engine";

/// The argv of `podvm run -d -i` for the instance `name`, with the work
/// directory `work`. The instance has no NIC; with an egress socket it has
/// one channel to it (T-109). The argv of the payload goes to the script
/// as positional parameters: no word of it is quoted again.
pub fn run_args(i: &Instance, env_file: &[String], env: &BTreeMap<String, String>, argv: &[String]) -> Vec<String> {
    let Instance { name, reference, work, egress, runner_root, limits, engine, init_image } = *i;
    let mut a: Vec<String> =
        ["run", "-d", "-i", "--name", name, "--podvm-tier=incus", "--podvm-net", "none"].map(String::from).to_vec();
    // The limits of the instance (T-162): incus `limits.memory`,
    // `limits.cpu` and `limits.processes`.
    if let Some(m) = limits.memory_bytes {
        a.extend(["--memory".into(), m.to_string()]);
    }
    if let Some(c) = limits.cpus {
        a.extend(["--cpus".into(), c.to_string()]);
    }
    if let Some(n) = limits.pids {
        a.extend(["--pids-limit".into(), n.to_string()]);
    }
    // The size of the root disk (T-482); podvm takes 64 MiB at least.
    if let Some(d) = limits.disk_bytes {
        a.extend(["--storage-opt".into(), format!("size={}", d.max(ROOT_MIN))]);
    }
    if init_image {
        // The image boots its own init and runs its own `docker.service`:
        // podvm gives the instance the nesting of an engine, and podrunner
        // mounts no engine and starts no `dockerd` (T-543).
        a.extend(["--podvm-init=image".into(), "--podvm-engine".into()]);
    } else if let Some(engine) = engine {
        a.push("--podvm-engine".into());
        a.push("-v".into());
        a.push(format!("{}:{ENGINE_MOUNT}:ro", engine.display()));
    }
    if let Some(egress) = egress {
        a.push("--podvm-channel".into());
        a.push(format!("{}:127.0.0.1:{EGRESS_PORT}", egress.display()));
        a.push("-e".into());
        a.push(format!("{CHANNEL_PORT_VAR}={EGRESS_PORT}"));
    }
    a.push("-v".into());
    a.push(format!("{}:{WORK_MOUNT}", work.display()));
    // The values of the environment in a file, not in the argv (T-405).
    a.extend(env_file.iter().cloned());
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
/// The name that a release before #273 gave: a 32-bit hash. A replay of the
/// journal after an upgrade removes it too.
pub fn legacy_instance_name(state_dir: &Path, unit: &str) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for b in state_dir.as_os_str().as_encoded_bytes() {
        h = (h ^ u32::from(*b)).wrapping_mul(0x0100_0193);
    }
    format!("podrunner-{h:08x}-{unit}")
}

pub fn instance_name(state_dir: &Path, unit: &str) -> String {
    // FNV-1a of 64 bits (#273): stable over builds, unlike the hasher of
    // the standard library; 32 bits met for two paths.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in state_dir.as_os_str().as_encoded_bytes() {
        h = (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("podrunner-{h:016x}-{unit}")
}
