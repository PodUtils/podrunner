#!/bin/bash
# SPDX-License-Identifier: 0BSD
# The capture of the hosted snapshot (T-542): run as the first step of a
# job on a GitHub-hosted label, before any action writes to the root.
#
#   bash scripts/hosted/capture.sh OUT
#
# It records the image (OUT/snapshot.json), stops the services that write,
# binds / to a view with no other mounts, writes the fix tree and the
# member list of the scrub (scrub.py), the layers (plan.py), and streams
# each layer through GNU tar, the check of the scrub (scrub.py verify) and
# zstd into OUT/layers/NN-NAME.tar.zst, with its diff id and its digest in
# OUT/layers.tsv. Check C1 (c1.py) compares the rows of the layers with
# the root after the scrub. The kernel layer `boot` (/boot and /boot/efi)
# is last.
# A failed check of any layer fails the capture. OUT must be outside the
# root file system (/mnt on a hosted runner).
set -euo pipefail
out=${1:?the directory of the output}
here=$(cd "$(dirname "$0")" && pwd)
view=/mnt/podrunner-root
level=${ZSTD_LEVEL:-10}

sudo mkdir -p "$out/layers"
sudo chown "$(id -u):$(id -g)" "$out" "$out/layers"

# The root of the hosted runner: Runner.Worker is an ancestor of this step.
runner_root=""
pid=$$
while [ "$pid" -gt 1 ]; do
  exe=$(readlink "/proc/$pid/exe" 2>/dev/null || true)
  case $exe in
    */bin/Runner.Worker)
      runner_root=${exe%/bin/Runner.Worker}
      break
      ;;
  esac
  pid=$(sed 's/.*) . \([0-9]*\) .*/\1/' "/proc/$pid/stat")
done
[ -n "$runner_root" ] || { echo "capture: no Runner.Worker among the ancestors" >&2; exit 1; }
runner_root=$(readlink -f "$runner_root")

# The record, before anything changes.
version=$(sed -n 's/^ImageVersion=//p' /etc/environment)
imageos=$(sed -n 's/^ImageOS=//p' /etc/environment)
python3 -I - "$out/snapshot.json" "$runner_root" "$version" "$imageos" <<'EOF'
import json, os, platform, sys
out, runner_root, version, imageos = sys.argv[1:5]
osr = dict(l.rstrip("\n").split("=", 1) for l in open("/etc/os-release") if "=" in l)
json.dump({
    "image_version": version, "image_os": imageos,
    "os": osr.get("ID", "").strip('"'), "os_version": osr.get("VERSION_ID", "").strip('"'),
    "kernel": platform.release(), "arch": platform.machine(),
    "cmdline": open("/proc/cmdline").read().strip(),
    "runner_root": runner_root,
    "label": os.environ.get("PODRUNNER_LABEL", ""), "run_id": os.environ.get("GITHUB_RUN_ID", ""),
}, open(out, "w"), indent=1)
EOF
cat "$out/snapshot.json"

# Quiesce: the services that write to the root and do not hold the job.
for s in docker.socket docker.service containerd.service snapd.socket snapd.service cron.service \
  sysstat.service unattended-upgrades.service rsyslog.service walinuxagent.service; do
  sudo systemctl stop "$s" 2>/dev/null || true
done
for s in $(systemctl list-units --type=service --state=running --no-legend --plain 'php*-fpm.service' | cut -d' ' -f1); do
  sudo systemctl stop "$s" || true
done
# Each timer: sysstat, apt-daily, logrotate and the others write during the
# capture, and check C1 then finds a file that changed between the passes.
for s in $(systemctl list-units --type=timer --state=active --no-legend --plain | cut -d' ' -f1); do
  sudo systemctl stop "$s" || true
done
sync

# A view of / with no other mount.
sudo mkdir -p "$view"
mountpoint -q "$view" || sudo mount --bind / "$view"
sudo mount --make-private "$view"

sudo python3 -I "$here/scrub.py" fix --root "$view" --out "$out/fix" --runner-root "${runner_root#/}" > "$out/fixed.txt"
sudo python3 -I "$here/scrub.py" list --root "$view" --fix "$out/fix" --runner-root "${runner_root#/}" > "$out/members"
sudo python3 -I "$here/plan.py" --root "$view" --members "$out/members" --out "$out/plan" \
  --manifest "$out/expected.tsv" | tee "$out/plan.tsv"

# One layer: a tar stream through the check of the scrub and zstd.
layer() {
  name=$1
  shift
  dst="$out/layers/$name.tar.zst"
  # No --acls until a podvm release takes the fix of pod-extract (COORD-228,
  # PodUtils/podvm#644): podvm 0.2.0-beta.37 refuses a PAX value with a
  # newline in an ACL record. The mode bits of each file stay. Put --acls
  # back with the podvm release that has the fix.
  sudo tar --format=posix --xattrs --xattrs-include='*' --sparse --numeric-owner \
    --pax-option=delete=atime,delete=ctime "$@" -cf - |
    tee >(sha256sum | cut -d' ' -f1 > "$dst.diff_id") \
      >(python3 -I "$here/scrub.py" verify --runner-root "${runner_root#/}" --rows "$dst.rows" 2> "$dst.verify"
        echo $? > "$dst.verify.rc") |
    zstd -q -T0 "-$level" -o "$dst"
  # The two checks of the tee end after zstd: wait for their files.
  for _ in $(seq 600); do [ -s "$dst.verify.rc" ] && [ -s "$dst.diff_id" ] && break; sleep 1; done
  printf '%s\t%s\tsha256:%s\t%s\t%s\n' "$name" "sha256:$(cat "$dst.diff_id")" \
    "$(sha256sum "$dst" | cut -d' ' -f1)" "$(stat -c %s "$dst")" "$(cat "$dst.verify.rc")" >> "$out/layers.tsv"
  if [ "$(cat "$dst.verify.rc")" != 0 ]; then
    echo "capture: the check of the scrub failed in the layer $name:" >&2
    cat "$dst.verify" >&2
    return 1
  fi
}

: > "$out/layers.tsv"
failed=0
for list in "$out"/plan/*.list; do
  name=$(basename "$list" .list)
  layer "$name" -C "$view" --null --no-verbatim-files-from --no-recursion -T "$list" || failed=1
done
# C1: the layers hold the root after the scrub, 1:1 (c1.py).
if ! python3 -I "$here/c1.py" compare "$out/expected.tsv" "$out"/layers/[0-9]*.rows > "$out/c1.txt"; then
  echo "capture: check C1 failed:" >&2
  head -n 50 "$out/c1.txt" >&2
  failed=1
fi
tail -n 1 "$out/c1.txt"
# The kernel layer: /boot with /boot/efi, at their paths.
layer boot -C / boot || failed=1
column -t "$out/layers.tsv" || cat "$out/layers.tsv"
exit "$failed"
