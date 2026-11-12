#!/bin/sh
# Measure `podvm run --podvm-confine` (T-079) on the rung of this host. Each
# check runs twice, with no spec and with a spec, and prints one line:
# "CHECK unconfined=RESULT confined=RESULT". RESULT is "reached" or
# "denied". A check proves the rules only when it is reached unconfined and
# denied confined; a path that the redirect of podvm hides already is
# denied both ways and proves nothing.
#
#   sh scripts/measure-podvm-confine.sh PODVM WORKDIR
#
# Run it as a uid with no user namespace (the supervise rung) and as root.
set -u
podvm=$1
work=$2
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$work"
export PODVM_STORE="$work/store"
"$podvm" probe 2>&1 | sed -n '2s/^/rung: /p' | cut -c1-100

python3 -I "$here/stand-in-image.py" "$work/image.tar" > /dev/null
"$podvm" load -i "$work/image.tar" > /dev/null 2>&1 || { echo "load no"; exit 1; }
image="glibc-mini:1"

# Host things that a payload must not reach: a file of this uid, and the
# environment of a process of this uid.
secret="$work/host-secret.txt"
echo podrunner-host-canary > "$secret"
PODRUNNER_ENV_CANARY=podrunner-env-canary sleep 120 &
victim=$!
trap 'kill $victim 2> /dev/null' EXIT

spec="read=/;write=/tmp"

check() {
  name=$1
  script=$2
  line="$name"
  for c in "" "--podvm-confine $spec"; do
    # shellcheck disable=SC2086
    out=$("$podvm" run --rm $c "$image" /bin/sh -c "$script" 2>&1)
    if printf '%s' "$out" | grep -q podrunner-; then r=reached; else r=denied; fi
    if [ -z "$c" ]; then line="$line unconfined=$r"; else line="$line confined=$r"; fi
  done
  echo "$line"
}

# The positive control: a write and a read in the allowed path.
check control "echo podrunner-own > /tmp/own && cat /tmp/own"
# A host file by its host path.
check host-file "cat $secret"
# The environment of a host process of this uid.
check host-environ "tr '\\0' '\\n' < /proc/$victim/environ"
# The root of a host process of this uid, through /proc.
check host-root "cat /proc/$victim/root$secret"
# The working directory of a host process, through /proc.
check host-cwd "cat /proc/$victim/cwd/host-secret.txt"
# An exec with no environment leaves the interposer of podvm (the escape of
# unit tests 25 and 26).
check escape-env-i "env -i /bin/sleep 0 && echo podrunner-escaped"
