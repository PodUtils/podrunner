#!/bin/bash
# Q1: does Runner.Listener / config.sh / run.sh start on this machine, and what do they need (ICU, OpenSSL, W^X)?
# Inputs: runner tarball pinned in lib.sh (sha256 baebf204...7c40). Offline: no network used.
# Expectation checked (exit 1 if false): with DOTNET_EnableWriteXorExecute=0 the listener prints 2.338.2 and exits 0.
. "$(dirname "$0")/lib.sh"
start_log 01-startup
conditions; need_runner
RUNNER_COPY=$(fresh_copy 01) || exit 2
cd "$RUNNER_COPY" || exit 2
fail=0
try() { # label, then env assignments and command
  local label=$1; shift
  for i in 1 2; do
    out=$(env "$@" 2>&1); rc=$?
    echo "--- [$label] run $i exit=$rc"; echo "$out" | head -${LINES_MAX:-6}
  done
}
echo "=== A. default environment (W^X on): listener --version"
try A ./bin/Runner.Listener --version
echo "=== B. DOTNET_EnableWriteXorExecute=0: listener --version"
try B DOTNET_EnableWriteXorExecute=0 ./bin/Runner.Listener --version
out=$(DOTNET_EnableWriteXorExecute=0 ./bin/Runner.Listener --version 2>&1); [ $? -eq 0 ] && [ "$out" = "2.338.2" ] || { echo "EXPECTATION FAILED: B"; fail=1; }
echo "=== C. W^X off: config.sh --help (first lines), run.sh --help (first lines)"
LINES_MAX=4 try C1 DOTNET_EnableWriteXorExecute=0 ./config.sh --help
LINES_MAX=4 try C2 DOTNET_EnableWriteXorExecute=0 ./run.sh --help
echo "=== D. W^X off + DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1"
try D DOTNET_EnableWriteXorExecute=0 DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1 ./bin/Runner.Listener --version
echo "=== E. ICU hidden (empty files bind-mounted over every libicu*.so.* in a private mount namespace; unshare -rm)"
mkdir -p "$M1"; : > "$M1/empty"
for i in 1 2; do
  out=$(unshare -rm bash -c 'for f in /usr/lib/x86_64-linux-gnu/libicu*.so*; do [ -f "$f" ] && mount --bind "$0/empty" "$f"; done; DOTNET_EnableWriteXorExecute=0 ./bin/Runner.Listener --version' "$M1" 2>&1); rc=$?
  echo "--- [E no-ICU, invariant unset] run $i exit=$rc"; echo "$out" | head -3
done
for i in 1 2; do
  out=$(unshare -rm bash -c 'for f in /usr/lib/x86_64-linux-gnu/libicu*.so*; do [ -f "$f" ] && mount --bind "$0/empty" "$f"; done; DOTNET_EnableWriteXorExecute=0 DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1 ./bin/Runner.Listener --version' "$M1" 2>&1); rc=$?
  echo "--- [E no-ICU, invariant=1] run $i exit=$rc"; echo "$out" | head -3
done
echo "=== F. what the runner's own dependency script would install on Debian-family (bin/installdependencies.sh, read only, not run: needs root)"
grep -n "install -y" bin/installdependencies.sh | sed -n 1,6p
echo "=== G. runtime config (self-contained, includedFrameworks)"
grep -n '"version"\|"tfm"' bin/Runner.Listener.runtimeconfig.json
echo "=== H. installed libs on this host: ICU, OpenSSL"
ls /usr/lib/x86_64-linux-gnu | grep -E '^(libicuuc|libssl|libcrypto|libgssapi_krb5|libz)\.so\.[0-9]+$'
exit $fail
