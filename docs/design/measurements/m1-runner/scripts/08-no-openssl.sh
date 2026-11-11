#!/bin/bash
# Q1 addendum: what does the runner do when OpenSSL (libssl/libcrypto) is not present? libssl is dlopen'ed by name at first use
# (see 06-layout.log), so `--version` may work while the JIT/OAuth path fails. Hide libssl*.so* and libcrypto*.so* with empty files
# bind-mounted in a private mount namespace (unshare -rm), then run the listener with the synthetic blob against the fake server.
# Control: same run without hiding must reach POST /session. Exit 1 if the control does not reach it.
. "$(dirname "$0")/lib.sh"
start_log 08-no-openssl
conditions; need_runner
export_wx_off
d=$(fresh_copy 08) || exit 2
: > "$M1/empty"
for variant in control hidden; do
  for i in 1 2; do
    start_server ok || exit 2
    python3 "$HERE/mkjit.py" "$PORT" "$SRVDIR" >/dev/null
    cd "$d" || exit 2
    export ACTIONS_RUNNER_INPUT_JITCONFIG=$(cat "$SRVDIR/jitconfig.b64")
    if [ $variant = hidden ]; then
      timeout 12 unshare -rm bash -c 'for f in /usr/lib/x86_64-linux-gnu/libssl*.so* /usr/lib/x86_64-linux-gnu/libcrypto*.so*; do [ -f "$f" ] && mount --bind "$0/empty" "$f"; done; exec ./bin/Runner.Listener run' "$M1" > "$SRVDIR/out.txt" 2>&1
    else
      timeout 6 ./bin/Runner.Listener run > "$SRVDIR/out.txt" 2>&1
    fi
    rc=$?
    unset ACTIONS_RUNNER_INPUT_JITCONFIG
    echo "--- [$variant] run $i exit=$rc (124 = still running at timeout)"
    head -6 "$SRVDIR/out.txt" | cut -c1-240
    echo "requests: $(summarize_reqlog "$REQLOG" | awk '{print $1" "$2" "$5}' | sort | uniq -c | tr '\n' ';')"
    [ $variant = control ] && { summarize_reqlog "$REQLOG" | grep -q '^POST   /session' || { echo "EXPECTATION FAILED: control did not reach POST /session"; fail=1; }; }
    rm -f "$d"/.runner "$d"/.credentials "$d"/.credentials_rsaparams
    stop_server
  done
done
rm -rf "$d"
pristine_intact || fail=1
exit ${fail:-0}
