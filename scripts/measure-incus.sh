#!/bin/sh
# Measure the incus tier of podvm for the incus kind (T-105, step 1): the
# runner image runs as a system container of incus, a volume takes a
# write back to the host, the user runner of the image runs its programs,
# and an instance that stays takes an exec and goes with rm.
#
#   sh scripts/measure-incus.sh PODVM STORE IMAGE WORKDIR
#
# The caller is in the group of the incus socket. It prints one line for
# each property: "incus NAME ok" or "incus NAME failed: why".
set -u
podvm=$1
store=$2
image=$3
work=$4
mkdir -p "$work/vol"
chmod 0777 "$work/vol"
rm -f "$work/vol/written"
export PODVM_STORE="$store"

start=$(date +%s)
out=$(timeout 600 "$podvm" run --rm --podvm-tier=incus -v "$work/vol:/mnt/vol" "$image" /bin/sh -c '
  test -x /home/runner/run.sh && echo "guest files ok"
  echo w > /mnt/vol/written && echo "guest wrote"
  stat -c "guest stat %n %U:%G %a" /home/runner /home/runner/run.sh
  if setpriv --reuid=runner --regid=runner --init-groups -- /home/runner/run.sh --version >/dev/null 2>&1; then echo "guest runner-run ok"; else echo "guest runner-run failed"; fi
  echo "guest net $(ls /sys/class/net | tr "\n" " ")"
  echo "guest boot $(cat /proc/sys/kernel/random/boot_id)"
' 2>"$work/stderr")
code=$?
end=$(date +%s)
echo "incus run exit=$code seconds=$((end - start))"
case "$out" in *"guest files ok"*) echo "incus files ok" ;; *) echo "incus files failed: $(tail -c 400 "$work/stderr" | tr '\n' ' ')" ;; esac
if [ -f "$work/vol/written" ]; then echo "incus volume ok"; else echo "incus volume failed: no file on the host"; fi
case "$out" in *"guest runner-run ok"*) echo "incus runner-run ok" ;; *) echo "incus runner-run failed" ;; esac
host_boot=$(cat /proc/sys/kernel/random/boot_id)
case "$out" in *"guest boot $host_boot"*) echo "incus kernel shared (a system container)" ;; *) echo "incus kernel own" ;; esac
printf '%s\n' "$out" | sed 's/^/incus out /'

# An instance that stays: run -d, exec, rm.
name="podrunner-incus-measure"
if "$podvm" run -d --podvm-tier=incus --name "$name" "$image" sleep 300 >/dev/null 2>"$work/stderr-d"; then
  if "$podvm" exec --podvm-tier=incus "$name" /bin/sh -c 'echo exec-42' 2>/dev/null | grep -q exec-42; then echo "incus exec ok"; else echo "incus exec failed"; fi
  if "$podvm" rm -f --podvm-tier=incus "$name" >/dev/null 2>&1; then echo "incus rm ok"; else echo "incus rm failed"; fi
else
  echo "incus run-d failed: $(tail -c 300 "$work/stderr-d" | tr '\n' ' ')"
fi
