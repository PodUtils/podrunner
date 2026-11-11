#!/bin/bash
# Q5: unit lifecycle. (a) detached payload that forks grandchildren: do they survive `stop -t 1` and `rm -f`?
# (b) cold and warm start cost of `podvm run --rm alpine true`.
# Marker: each grandchild has a unique sleep duration in argv; /proc is scanned by cmdline. 3 repetitions per image.
# Exit 0 = ran; 1 = a grandchild survived (the issue's 60 s destroy acceptance depends on none surviving); 2 = no podvm.
. "$(dirname "$0")/lib.sh"; need
echo "date=$(date -u +%FT%TZ) kernel=$(uname -r) cpus=$(nproc) virt=$(systemd-detect-virt) podvm=$("$PODVM" --version)"
scan() { # scan N : print pid ppid state of every process with the token N or N.5 as an argv word (the loader family
  # puts the applet name and the binary path before the arguments, so a "sleep N" substring would miss busybox children)
  local d c n=0
  for d in /proc/[0-9]*; do
    c=$(tr '\0' ' ' <"$d/cmdline" 2>/dev/null) || continue
    case "$c" in *" $1 "*|*" $1.5 "*) n=$((n+1)); echo "   pid=${d#/proc/} ppid=$(awk '/^PPid/{print $2}' "$d/status" 2>/dev/null) state=$(awk '/^State/{print $2}' "$d/status" 2>/dev/null) argv_tail=...${c: -30}";; esac
  done; SCAN_N=$n; }
reap() { local d c; for d in /proc/[0-9]*; do c=$(tr '\0' ' ' <"$d/cmdline" 2>/dev/null) || continue; case "$c" in *" $1 "*|*" $1.5 "*) kill -9 "${d#/proc/}" 2>/dev/null;; esac; done; }
"$PODVM" pull alpine >/dev/null 2>&1; "$PODVM" pull debian:stable-slim >/dev/null 2>&1
SURV=0
for img in debian:stable-slim alpine; do
  for rep in 1 2 3; do
    N=$((7000 + RANDOM % 900 + 1000*rep))
    id=$("$PODVM" run -d -q "$img" sh -c "sleep $N & sleep $N.5 & wait" 2>/dev/null)
    sleep 1.5
    echo "[$img rep$rep] id=${id:0:12} before stop:"; scan "$N"; before=$SCAN_N
    s=$(date +%s.%N); "$PODVM" stop -t 1 "$id" >/dev/null 2>&1; src=$?; e=$(date +%s.%N)
    sleep 1; echo "[$img rep$rep] stop exit=$src wall=$(awk "BEGIN{printf \"%.2f\",$e-$s}")s; after stop:"; scan "$N"; a1=$SCAN_N
    "$PODVM" rm -f "$id" >/dev/null 2>&1; rrc=$?; sleep 1
    echo "[$img rep$rep] rm -f exit=$rrc; after rm:"; scan "$N"; a2=$SCAN_N
    echo "[$img rep$rep] summary: processes with marker before=$before after_stop=$a1 after_rm=$a2"
    [ "$a2" -gt 0 ] && SURV=1
    reap "$N"
  done
done
echo "## control: a payload with no grandchild leaves nothing (marker in the payload's own argv)"
for rep in 1 2; do
  N=$((8000 + RANDOM % 900)); id=$("$PODVM" run -d -q debian:stable-slim sleep $N 2>/dev/null); sleep 1; scan "$N" >/dev/null; b=$SCAN_N
  "$PODVM" stop -t 1 "$id" >/dev/null 2>&1; sleep 1; scan "$N" >/dev/null; echo "control rep$rep: before=$b after_stop=$SCAN_N"; "$PODVM" rm -f "$id" >/dev/null 2>&1; reap "$N"
done
echo "## start cost: podvm run --rm alpine true"
echo "cold = empty store (rmi alpine, includes pull through this host's proxy and extraction); warm = image in store"
for rep in 1 2 3; do
  "$PODVM" rmi alpine >/dev/null 2>&1
  s=$(date +%s.%N); "$PODVM" run --rm -q alpine true >/dev/null 2>&1; rc=$?; e=$(date +%s.%N)
  echo "cold rep$rep exit=$rc wall=$(awk "BEGIN{printf \"%.2f\",$e-$s}")s"
done
for rep in 1 2 3; do
  s=$(date +%s.%N); "$PODVM" run --rm -q alpine true >/dev/null 2>&1; rc=$?; e=$(date +%s.%N)
  echo "warm rep$rep exit=$rc wall=$(awk "BEGIN{printf \"%.2f\",$e-$s}")s"
done
echo "warm, no --rm (rootfs already extracted):"
for rep in 1 2 3; do
  s=$(date +%s.%N); "$PODVM" run -q alpine true >/dev/null 2>&1; rc=$?; e=$(date +%s.%N)
  echo "warm-norm rep$rep exit=$rc wall=$(awk "BEGIN{printf \"%.2f\",$e-$s}")s"
done
"$PODVM" rm -f $("$PODVM" ps -aq 2>/dev/null) >/dev/null 2>&1
exit $SURV
