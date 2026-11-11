#!/bin/bash
# Q2: RLIMIT_FSIZE vs .NET W^X (doublemapper memfd ftruncate). Verifies or refutes the archive claim (2 TiB memfd truncate).
# Matrix: fsize in {20 GiB (this host's hard cap), 1 GiB, 128 MiB, 1 MiB} x {W^X default, DOTNET_EnableWriteXorExecute=0, W^X default + SIGXFSZ ignored}.
# Targets: Runner.Listener --version, and Runner.Worker spawnclient 0 0 (starts .NET, fails later on a bad pipe handle: that is the expected non-zero).
# "unlimited" and any value above the hard limit cannot be set by an unprivileged process on this host; the script records the refusal.
# Exit 1 if the 2 TiB ftruncate is not seen under strace, 0 otherwise.
. "$(dirname "$0")/lib.sh"
start_log 02-fsize
conditions; need_runner; command -v strace >/dev/null || { echo "strace missing (sudo apt-get install -y strace)"; exit 2; }
RUNNER_COPY=$(fresh_copy 02) || exit 2
cd "$RUNNER_COPY" || exit 2
sig() { local rc=$1; if [ "$rc" -gt 128 ]; then echo "signal $((rc-128)) ($(kill -l $((rc-128))))"; else echo "no signal"; fi; }
fail=0
echo "=== 0. can this process raise the fsize limit above its hard limit?"
prlimit --fsize=unlimited true; echo "prlimit --fsize=unlimited (unprivileged) exit=$?"
sudo -n prlimit --fsize=unlimited true 2>&1; echo "sudo prlimit --fsize=unlimited exit=$? (container has no CAP_SYS_RESOURCE if this is non-zero)"
grep CapEff /proc/self/status
echo "=== 1. matrix"
for n in 21474836480 1073741824 134217728 1048576; do
  echo "--- limit as seen by the child: $(prlimit --fsize=$n cat /proc/self/limits | grep 'Max file size')"
  for mode in wx-default wx-off xfsz-ignored; do
    for target in listener worker; do
      for i in 1 2; do
        case $target in
          listener) cmd=(./bin/Runner.Listener --version) ;;
          worker)   cmd=(./bin/Runner.Worker spawnclient 0 0) ;;
        esac
        case $mode in
          wx-default)   out=$(prlimit --fsize=$n "${cmd[@]}" </dev/null 2>&1) ;;
          wx-off)       out=$(DOTNET_EnableWriteXorExecute=0 prlimit --fsize=$n "${cmd[@]}" </dev/null 2>&1) ;;
          xfsz-ignored) out=$( trap '' XFSZ; prlimit --fsize=$n "${cmd[@]}" </dev/null 2>&1) ;;
        esac
        rc=$?
        printf 'fsize=%-11s mode=%-12s target=%-8s run=%s exit=%-3s %s first-line="%s"\n' "$n" "$mode" "$target" "$i" "$rc" "$(sig $rc)" "$(echo "$out" | head -1 | cut -c1-80)"
      done
    done
  done
done
echo "=== 2. strace of the W^X-default run at the 20 GiB host limit (ftruncate, memfd_create, fallocate)"
strace -f -e trace=ftruncate,memfd_create,fallocate -o "$M1/st.txt" ./bin/Runner.Listener --version; echo "listener exit=$?"
cat "$M1/st.txt"
grep -q 'ftruncate(8, 2199023255552)' "$M1/st.txt" || { echo "EXPECTATION FAILED: no ftruncate of 2199023255552 on fd 8"; fail=1; }
echo "2199023255552 = $((2199023255552 / 1024 / 1024 / 1024 / 1024)) TiB exactly"
echo "=== 3. same, W^X off"
DOTNET_EnableWriteXorExecute=0 strace -f -e trace=ftruncate,memfd_create,fallocate -o "$M1/st0.txt" ./bin/Runner.Listener --version; echo "listener exit=$?"
grep -v 'exited with' "$M1/st0.txt"
echo "=== 4. same, SIGXFSZ ignored, 1 MiB limit"
( trap '' XFSZ; prlimit --fsize=1048576 strace -f -e trace=ftruncate,memfd_create,fallocate -o "$M1/st1.txt" ./bin/Runner.Listener --version; echo "listener exit=$?" )
grep -v 'exited with' "$M1/st1.txt"
exit $fail
