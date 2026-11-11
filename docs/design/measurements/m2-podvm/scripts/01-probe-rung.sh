#!/bin/bash
# Q1: which rung does podvm select on this machine and why; can the userland rung be forced; what do the
# forcing knobs (PODVM_MODE, --podvm-tier) do when chroot is denied.
# Expectation checked (exit 1 if false): probe selects `supervise` here and the first run enters `userland`
# on the loader family, i.e. this machine already is the chroot-denied class.
. "$(dirname "$0")/lib.sh"; need
echo "date=$(date -u +%FT%TZ) kernel=$(uname -r) cpus=$(nproc) virt=$(systemd-detect-virt) uid=$(id -u)"
echo "podvm=$("$PODVM" --version)"
echo "## doctor"; "$PODVM" doctor; echo "doctor exit=$?"
echo "## probe (text, head)"; "$PODVM" probe 2>&1 | head -8; echo "probe exit=${PIPESTATUS[0]}"
echo "## probe --json (selected fields)"
"$PODVM" probe --json >"$WORK/probe.json"; echo "probe --json exit=$?"
python3 - "$WORK/probe.json" <<'P'
import json,sys
d=json.load(open(sys.argv[1]))
print("rung=",d["rung"]); print("rung_must_never_claim=",d["rung_must_never_claim"])
print("rejected=",json.dumps(d["rejected"]))
for p in d["probes"]:
    if p["name"] in ("chroot(/tmp)","unshare(CLONE_NEWNS)","unshare(CLONE_NEWUSER)","clone(CLONE_NEWUSER)","mount(tmpfs,/mnt)","memfd_create+exec","open(/dev/fuse, O_RDWR)","mknod(chr 1:3 /tmp/nodprobe)"):
        print("probe",p["name"],p["verdict"],p["errno_name"])
print("tiers.supervise.refusal=",d["tiers"]["supervise"]["refusal"])
P
echo "## system info .Rung|.EnteredRung"; "$PODVM" system info --format '{{.Rung}}|{{.EnteredRung}}'; echo "exit=$?"
echo "## pull alpine"; "$PODVM" pull alpine >/dev/null 2>&1; echo "pull exit=$?"
echo "## default run banner (--verbose)"
"$PODVM" run --rm --verbose alpine true 2>&1 | grep -E "^podvm 0|mode=|entering without|would permit" | cut -c1-260
echo "default-run exit=${PIPESTATUS[0]}"
echo "## forcing attempts"
for m in memfd rundir cache fuse tmpfs chroot userland; do
  PODVM_MODE=$m "$PODVM" run --rm -q alpine true >"$WORK/o.txt" 2>"$WORK/e.txt"; rc=$?
  echo "PODVM_MODE=$m exit=$rc :: $(grep -E '^podvm run' "$WORK/e.txt" | tail -1 | cut -c1-330)"
done
"$PODVM" run --rm --verbose --podvm-tier=chroot alpine true 2>&1 | grep -E "mode=" ; echo "--podvm-tier=chroot exit=${PIPESTATUS[0]}"
"$PODVM" run --rm -q --podvm-tier=machine alpine true 2>&1 | tail -1; echo "--podvm-tier=machine exit=${PIPESTATUS[0]}"
# expectation
[ "$(python3 -c "import json;print(json.load(open('$WORK/probe.json'))['rung'])")" = supervise ] || exit 1
"$PODVM" run --rm --verbose alpine true 2>&1 | grep -q "mode=userland" || exit 1
exit 0
