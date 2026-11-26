//! The argv of `podvm run` for one guest, the script that the guest runs
//! first, and the private directory of the credential files.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use podrunner_schema::{Credential, FileBytes, PlainName};
use podrunner_unit::{UnitCode, UnitError};

use crate::cache::CACHE_MOUNT;

/// Where the guest sees the credential directory (a 9p mount, read-only).
pub const CRED_MOUNT: &str = "/mnt/podrunner-cred";

/// Where the guest sees the work directory of the unit (a 9p mount): the
/// payload runs there, so a file of the job lands in the unit directory
/// of the host, and `destroy` removes it.
pub const WORK_MOUNT: &str = "/podrunner-work";

/// Where the guest sees the directory of the static Docker binaries of the
/// operator (a 9p mount, read-only), when the kind has an engine (T-103).
pub const ENGINE_MOUNT: &str = "/podrunner-engine";

/// The socket of the engine in the guest: the runner gets it as
/// `DOCKER_HOST`. It is in the guest only (G-09).
pub const ENGINE_SOCKET: &str = "/run/podrunner-docker.sock";

/// The awk program of the mount check (T-036, GitHub #36): it fails when a
/// host mount of the guest (9p or virtiofs) is not the credential
/// directory or the engine directory, read-only, or the work directory of
/// the unit.
pub const MOUNT_CHECK: &str = r#"$3 == "9p" || $3 == "virtiofs" { if (($2 == "/mnt/podrunner-cred" || $2 == "/podrunner-engine") && ("," $4 ",") ~ /,ro,/) next; if ($2 == "/podrunner-work") next; bad = 1 } END { exit bad }"#;

/// [`MOUNT_CHECK`] of a guest with the cache of its job (T-648): it also
/// takes the cache, and for a `ro` cache only a read-only mount.
pub fn mount_check(cache: Option<bool>) -> String {
    let take = match cache {
        None => return MOUNT_CHECK.into(),
        Some(true) => r#"if ($2 == "/podrunner-cache" && ("," $4 ",") ~ /,ro,/) next; "#,
        Some(false) => r#"if ($2 == "/podrunner-cache") next; "#,
    };
    MOUNT_CHECK.replacen("bad = 1", &format!("{take}bad = 1"), 1)
}

/// The script of the guest. `$1` is the runner root; the rest is the argv of
/// the payload. It copies the credential files into the runner root at
/// mode 0600, gives them to the user of the image (`runner`, when it
/// exists: the official runner refuses root), and runs the payload in the
/// work directory.
pub(crate) const SCRIPT: &str = r#"set -eu
root=$1; shift
# Each host mount of the guest is in the table of the unit (T-036).
if ! awk 'MOUNT_CHECK' /proc/mounts; then
  echo "podrunner: a host mount of the guest is outside the table of the unit" >&2
  exit 96
fi
# The root of the guest is the initramfs on a tmpfs, and the kernel caps it
# at half of the memory less the initrd: a job of the larger image in 8G
# got less than 1 GiB (T-688). The job then gets the memory that its
# processes do not use, as the disk of a hosted runner.
if awk '$2 == "/" && $3 == "rootfs" { f = 1 } END { exit !f }' /proc/mounts; then
  mount -o remount,size=75% / || echo "podrunner: the root of the guest keeps its size" >&2
fi
# A warm guest (T-487) waits for its claim: the environment and the argv
# of the job come in env.sh, before the engine starts, so dockerd gets the
# variables of the job. The claim directory is no regular file: the copy
# below skips it.
if [ "${PODRUNNER_WARM:-}" = 1 ]; then
  : > /podrunner-work/.podrunner-warm
  while [ ! -e /mnt/podrunner-cred/.podrunner-claim/go ]; do sleep 0.1; done
  rm -f /podrunner-work/.podrunner-warm
  unset PODRUNNER_WARM
  . /mnt/podrunner-cred/.podrunner-claim/env.sh
fi
# The channel to the egress socket: the proxy of the payload. The script
# sets the variables in the guest, as podvm rewrites a loopback proxy
# variable of its own -e to the gateway of a NIC that this guest lacks.
if [ -n "${PODRUNNER_CHANNEL_PORT:-}" ]; then
  u=http://127.0.0.1:$PODRUNNER_CHANNEL_PORT
  export http_proxy=$u https_proxy=$u HTTP_PROXY=$u HTTPS_PROXY=$u
  # The fetch of Node takes the proxy only with this (#272).
  export NODE_USE_ENV_PROXY="${NODE_USE_ENV_PROXY:-1}"
fi
# The engine of the unit (T-103): dockerd as root in the guest, its socket
# in the guest only, the proxy of the channel for its pulls. The root of
# the guest is an initramfs: DOCKER_RAMDISK keeps runc off pivot_root.
# With a channel, the broker is the mirror of Docker Hub: the pinned images
# come from the store of the run (T-157); a miss goes to Docker Hub.
if [ -x /podrunner-engine/dockerd ]; then
  mirror=
  if [ -n "${PODRUNNER_CHANNEL_PORT:-}" ]; then
    mirror="--registry-mirror=http://broker.podrunner.test --insecure-registry=broker.podrunner.test"
  fi
  # shellcheck disable=SC2086
  PATH=/podrunner-engine:$PATH DOCKER_RAMDISK=true dockerd --host=unix:///run/podrunner-docker.sock $mirror >/tmp/podrunner-dockerd.log 2>&1 &
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
  # setpriv keeps the environment of root: the home of the user (T-178).
  home=$(getent passwd runner | cut -d: -f6)
  export HOME="${home:-/home/runner}" USER=runner LOGNAME=runner
  exec setpriv --reuid=runner --regid=runner --init-groups -- "$@"
fi
exec "$@"
"#;

/// podvm's report of the run of a guest, in the directory of the unit.
pub const REPORT: &str = "run-report.json";

/// The port in the guest of the channel to the egress socket: the payload
/// gets `http://127.0.0.1:EGRESS_PORT` as its proxy.
pub const EGRESS_PORT: u16 = 3128;

/// The size and the channel of one guest.
pub struct Shape<'a> {
    pub mem: &'a str,
    pub cpus: u32,
    /// The egress socket of the unit on the host: the only way out of the
    /// guest, which has no NIC.
    pub egress: Option<&'a Path>,
    /// The directory of the static Docker binaries: the guest runs an
    /// engine of its own (`--podvm-engine`, T-103).
    pub engine: Option<&'a Path>,
    /// The selected payload contains the guest engine.
    pub payload_engine: bool,
    /// The most processes of the payload (`--pids-limit`, T-488): the init
    /// of the guest puts the command in a cgroup with `pids.max`.
    pub pids: Option<u32>,
    /// The guest boots the init of the image (`--podvm-init=image`, T-549).
    /// The machine tier takes it in a foreground run only: this kind runs
    /// no `run -d`.
    pub init_image: bool,
    /// The cache of the job (T-648): its host directory, and true when
    /// read-only.
    pub cache: Option<(&'a Path, bool)>,
}

/// The variable that gives the guest script the port of the egress
/// channel; the script makes the proxy variables of the payload from it
/// (`http_proxy`, `https_proxy` and their upper-case forms).
pub const CHANNEL_PORT_VAR: &str = "PODRUNNER_CHANNEL_PORT";

/// The argv of `podvm run` for one guest, with the credential directory
/// `UNIT_DIR/cred` and the work directory `UNIT_DIR/work`. The guest has no
/// NIC; with an egress socket it has one channel to it. The argv of the
/// payload goes to the script as positional parameters: no word of it is
/// quoted again.
pub fn run_args(
    reference: &str,
    unit_dir: &Path,
    shape: &Shape,
    runner_root: &str,
    env_file: &[String],
    env: &BTreeMap<String, String>,
    argv: &[String],
) -> Vec<String> {
    let mut a: Vec<String> = ["run", "--rm", "--podvm-tier=machine", "--podvm-net", "none", "--podvm-mem", shape.mem]
        .map(String::from)
        .to_vec();
    a.push("--podvm-cpus".into());
    a.push(shape.cpus.to_string());
    // podvm's report of the run, for the OOM kills of the guest (T-515).
    a.push("--report".into());
    a.push(unit_dir.join(REPORT).display().to_string());
    if let Some(n) = shape.pids {
        a.extend(["--pids-limit".into(), n.to_string()]);
    }
    if shape.init_image {
        a.push("--podvm-init=image".into());
    }
    let payload_engine = shape.payload_engine && shape.engine.is_none() && !shape.init_image;
    if let Some(engine) = shape.engine {
        a.push("--podvm-engine".into());
        a.push("-v".into());
        a.push(format!("{}:{ENGINE_MOUNT}:ro", engine.display()));
    } else if payload_engine {
        a.push("--podvm-engine".into());
    }
    if let Some(egress) = shape.egress {
        a.push("--podvm-channel".into());
        a.push(format!("{}:127.0.0.1:{EGRESS_PORT}", egress.display()));
        a.push("-e".into());
        a.push(format!("{CHANNEL_PORT_VAR}={EGRESS_PORT}"));
    }
    a.push("-v".into());
    a.push(format!("{}:{CRED_MOUNT}:ro", unit_dir.join("cred").display()));
    a.push("-v".into());
    a.push(format!("{}:{WORK_MOUNT}", unit_dir.join("work").display()));
    if let Some((host, ro)) = shape.cache {
        a.push("-v".into());
        a.push(format!("{}:{CACHE_MOUNT}{}", host.display(), if ro { ":ro" } else { "" }));
    }
    // The values of the environment in a file, not in the argv (T-405).
    a.extend(env_file.iter().cloned());
    for (k, v) in env {
        a.push("-e".into());
        a.push(format!("{k}={v}"));
    }
    a.push(reference.into());
    let script =
        if payload_engine { podrunner_unit::payload_engine_script(SCRIPT, ENGINE_MOUNT) } else { SCRIPT.into() };
    let check = mount_check(shape.cache.map(|(_, ro)| ro));
    let script = script.replace("'MOUNT_CHECK'", &format!("'{check}'"));
    a.extend(["/bin/sh".to_string(), "-c".into(), script, "podrunner".into(), runner_root.into()]);
    a.extend(argv.iter().cloned());
    a
}

/// Make the work directory: mode 0777, so the user of the image writes in
/// it over 9p. Its parent, the unit directory, is 0700: no other user of
/// the host reaches it.
pub(crate) fn make_work(unit_dir: &Path, work: &Path) -> Result<(), UnitError> {
    let host = |e: std::io::Error| UnitError::new(UnitCode::Host, format!("the work directory: {}", e.kind()));
    fs::create_dir_all(work).map_err(host)?;
    fs::set_permissions(unit_dir, fs::Permissions::from_mode(0o700)).map_err(host)?;
    fs::set_permissions(work, fs::Permissions::from_mode(0o777)).map_err(host)
}

/// Write each credential file into `dir` (mode 0700) at mode 0600. A file
/// name that is not plain is refused.
pub(crate) fn write_credentials(dir: &Path, credential: &Credential) -> Result<(), UnitError> {
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
        let bytes = f.bytes().map_err(|e| UnitError::new(UnitCode::UnitStartFailed, e))?;
        out.write_all(&bytes).map_err(|e| host("a credential file", e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    /// T-688: the root of a guest is the initramfs on a tmpfs with half of the
    /// memory less the initrd; the script raises its cap. A root of another
    /// type keeps its size, and a refused remount does not end the guest.
    #[test]
    fn the_guest_script_raises_the_cap_of_an_initramfs_root() {
        let d = crate::tests::dir("root-size");
        let script = super::SCRIPT;
        let from = script.find("if awk '$2 == \"/\" && $3 == \"rootfs\"").unwrap();
        let block = &script[from..from + script[from..].find("fi\n").unwrap() + 3];
        let block = block.replace("/proc/mounts", &d.join("mounts").display().to_string());
        let bin = d.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let run = |table: &str, mount_rc: u8| {
            fs::write(d.join("mounts"), table).unwrap();
            let fake = format!("#!/bin/sh\necho \"$*\" > '{}'\nexit {mount_rc}\n", d.join("argv").display());
            fs::write(bin.join("mount"), fake).unwrap();
            fs::set_permissions(bin.join("mount"), fs::Permissions::from_mode(0o755)).unwrap();
            let _ = fs::remove_file(d.join("argv"));
            let out = std::process::Command::new("sh")
                .args(["-c", &format!("set -eu; {block} echo after")])
                .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
                .output()
                .unwrap();
            let argv = fs::read_to_string(d.join("argv")).unwrap_or_default();
            (String::from_utf8_lossy(&out.stdout).trim().to_string(), argv.trim().to_string(), out.stderr)
        };
        let (out, argv, _) = run("rootfs / rootfs rw,size=1572864k 0 0\nproc /proc proc rw 0 0\n", 0);
        assert_eq!((out.as_str(), argv.as_str()), ("after", "-o remount,size=75% /"));
        let (out, argv, _) = run("/dev/vda / ext4 rw 0 0\nrootfs /mnt rootfs rw 0 0\n", 0);
        assert_eq!((out.as_str(), argv.as_str()), ("after", ""), "a root on a disk keeps its size");
        let (out, _, err) = run("rootfs / rootfs rw 0 0\n", 32);
        assert_eq!(out, "after", "a refused remount does not end the guest");
        assert!(String::from_utf8_lossy(&err).contains("keeps its size"));
        let _ = fs::remove_dir_all(&d);
    }
}
