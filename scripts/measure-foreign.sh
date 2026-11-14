#!/bin/sh
# Measure a guest of another architecture in the machine tier of podvm
# (T-151): an image of linux/arm64 on an x86_64 host, emulated by QEMU
# with no KVM. It prints one line for each property:
# "foreign NAME ..." with the exit, the seconds and the machine of the guest.
#
#   sh scripts/measure-foreign.sh PODVM WORKDIR [PLATFORM] [IMAGE]
#
# A measure: each failure is printed, and the exit is 0.
set -u
podvm=$1
work=$2
platform=${3:-linux/arm64}
image=${4:-busybox:1.37.0}
mkdir -p "$work/store"
export PODVM_STORE="$work/store"

start=$(date +%s)
if ! timeout 300 "$podvm" pull -q --platform "$platform" "$image" >"$work/pull.out" 2>&1; then
  echo "foreign pull failed: $(tail -c 300 "$work/pull.out" | tr '\n' ' ')"
  exit 0
fi
echo "foreign pull ok seconds=$(($(date +%s) - start)) platform=$platform image=$image"

start=$(date +%s)
out=$(timeout 900 "$podvm" run --rm --podvm-tier=machine --podvm-mem 1G --podvm-cpus 2 --platform "$platform" "$image" \
  /bin/sh -c 'uname -m; echo guest-ran-$((6*7))' 2>"$work/run.err")
code=$?
end=$(date +%s)
accel=$(grep -o 'kvm\|tcg' "$work/run.err" | head -1)
echo "foreign boot exit=$code seconds=$((end - start)) accel=${accel:-unknown}"
case "$out" in
  *guest-ran-42*) echo "foreign job ok machine=$(echo "$out" | head -1)" ;;
  *) echo "foreign job failed: $(tail -c 400 "$work/run.err" | tr '\n' ' ')" ;;
esac
exit 0
