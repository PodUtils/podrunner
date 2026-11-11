#!/bin/bash
# Q6: do containers of one image share one mutable rootfs? A polls /etc/marker for 10 s; B writes /etc/marker; C reads it.
# Each scenario first clears /etc/marker (see fresh()) so a marker left by an earlier scenario cannot count.
# Scenarios vary --rm on A, B, C. Then the sequential controls (no concurrency): B alone then C, with and without --rm.
# Last: concurrent --add-host of the same name with different IPs.
# Exit 0 = ran. 1 = B's write was NOT visible to A (the R1 finding is refuted). 2 = no podvm.
. "$(dirname "$0")/lib.sh"; need
echo "date=$(date -u +%FT%TZ) kernel=$(uname -r) cpus=$(nproc) virt=$(systemd-detect-virt) podvm=$("$PODVM" --version)"
POLL='for i in 1 2 3 4 5 6 7 8 9 10; do if [ -e /etc/marker ]; then echo "A saw /etc/marker at t=$i: $(cat /etc/marker)"; fi; sleep 1; done; echo A-end; ls /etc/marker 2>&1'
fresh() { # make sure alpine is in the store and /etc/marker is absent. `rmi` is not enough: it leaves the extracted rootfs (found by the first run of this script).
  "$PODVM" pull alpine >/dev/null 2>&1 || { echo "pull failed"; exit 2; }
  "$PODVM" run -q alpine rm -f /etc/marker >/dev/null 2>&1
  if "$PODVM" run -q alpine ls /etc/marker >/dev/null 2>&1; then echo "marker could not be cleared"; exit 2; fi; }
ASAW=0
conc() { # conc LABEL Aflag Bflag Cflag
  fresh
  echo "######## $1 (A:${2:-none} B:${3:-none} C:${4:-none})"
  "$PODVM" run $2 -q alpine sh -c "$POLL" >"$WORK/A.out" 2>"$WORK/A.err" & ap=$!
  sleep 3
  "$PODVM" run $3 -q alpine sh -c 'echo from-B > /etc/marker; echo B-wrote' >"$WORK/B.out" 2>"$WORK/B.err"; echo "B exit=$? stdout=$(cat "$WORK/B.out")"
  "$PODVM" run $4 -q alpine sh -c 'cat /etc/marker 2>&1; echo C-end' >"$WORK/C.out" 2>"$WORK/C.err"; echo "C (started while A still runs) exit=$? stdout=$(tr '\n' '|' <"$WORK/C.out")"
  wait $ap; echo "A exit=$?"; sed 's/^/   A: /' "$WORK/A.out" | head -4; echo "   A: ... ($(grep -c saw "$WORK/A.out") lines with the marker)"
  grep -q saw "$WORK/A.out" && ASAW=1
  "$PODVM" run $4 -q alpine sh -c 'cat /etc/marker 2>&1' 2>/dev/null | sed 's/^/D (after all ended, flags C:'"${4:-none}"'): /'
}
conc T1 --rm --rm --rm
conc T2 "" "" ""
conc T3 "" "" --rm
echo "######## sequential controls (no concurrency)"
for clear in no yes; do for bf in "" --rm; do for cf in "" --rm; do
  fresh
  if [ $clear = yes ]; then "$PODVM" rm -f $("$PODVM" ps -aq 2>/dev/null) >/dev/null 2>&1; fi
  recs=$("$PODVM" ps -aq 2>/dev/null | wc -l)
  "$PODVM" run $bf -q alpine sh -c 'echo from-B > /etc/marker' >/dev/null 2>&1; b=$?
  out=$("$PODVM" run $cf -q alpine sh -c 'cat /etc/marker 2>&1' 2>/dev/null)
  echo "records-cleared-before-B=$clear (container records at B start: $recs) B:${bf:-none} exit=$b ; later C:${cf:-none} reads: ${out:-<empty>}"
done; done; done
echo "######## /etc/hosts collision: two concurrent runs add the same name with different IPs"
fresh
"$PODVM" run -q --add-host svc:10.0.0.1 alpine sh -c 'sleep 4; grep svc /etc/hosts' >"$WORK/H1.out" 2>/dev/null & h1=$!
sleep 1.5
"$PODVM" run -q --add-host svc:10.0.0.2 alpine sh -c 'grep svc /etc/hosts' >"$WORK/H2.out" 2>/dev/null; echo "run2 exit=$? sees: $(tr '\n' '|' <"$WORK/H2.out")"
wait $h1; echo "run1 (started first, read after run2 started) sees: $(tr '\n' '|' <"$WORK/H1.out")"
"$PODVM" rm -f $("$PODVM" ps -aq 2>/dev/null) >/dev/null 2>&1
[ $ASAW = 1 ] || exit 1
exit 0
