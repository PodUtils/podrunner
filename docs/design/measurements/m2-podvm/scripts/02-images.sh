#!/bin/bash
# Q2: pull alpine, debian:stable-slim, node:lts-slim through `podvm run --rm` and run the brief's payloads on the
# default rung (userland on this host, see 01) and on the memfd family (PODVM_MODE=memfd); a static hello built with
# zig is the one payload the memfd family accepts. Also the node fetch probe with port 1 (as briefed) and port 2.
# Pinned inputs: images by tag (digests are printed in the log). Exit 0 = ran; 1 = a default-rung expectation failed;
# 2 = could not pull.
. "$(dirname "$0")/lib.sh"; need
echo "date=$(date -u +%FT%TZ) kernel=$(uname -r) cpus=$(nproc) virt=$(systemd-detect-virt)"
for i in alpine debian:stable-slim node:lts-slim; do
  "$PODVM" pull "$i" 2>&1 | grep -E "Digest|rror" ; [ "${PIPESTATUS[0]}" = 0 ] || exit 2
done
FAIL=0
chk() { [ "$LAST_RC" = "$1" ] || FAIL=1; }
echo "## default rung"
R="$PODVM run --rm -q"
run_case alpine-true $R alpine true; chk 0
run_case alpine-sh-echo $R alpine sh -c 'echo ok'; chk 0
run_case debian-bash-echo $R debian:stable-slim bash -c 'echo ok'; chk 0
run_case node-log1 $R node:lts-slim node -e "console.log(1)"; chk 0
run_case node-fetch-port1 $R node:lts-slim node -e "fetch('http://127.0.0.1:1').catch(e=>console.log(e.code))"; chk 0
run_case node-fetch-port2 $R node:lts-slim node -e "fetch('http://127.0.0.1:2').catch(e=>console.log(e.code, e.cause&&e.cause.code))"; chk 0
if command -v node >/dev/null; then
  echo "control (host node $(node -v), no podvm):"
  node -e "fetch('http://127.0.0.1:1').catch(e=>console.log('port1',e.code,String(e.cause)))"
  node -e "fetch('http://127.0.0.1:2').catch(e=>console.log('port2',e.code,e.cause&&e.cause.code))"
fi
run_case node-version $R node:lts-slim node -v
echo "## default-rung banner, glibc image"
$PODVM run --rm --verbose debian:stable-slim true 2>&1 | grep -E "mode=|interpose: /|libc=" | cut -c1-200
echo "## memfd family (PODVM_MODE=memfd)"
export PODVM_MODE=memfd
run_case memfd-alpine-true $R alpine true
run_case memfd-debian-bash $R debian:stable-slim bash -c 'echo ok'
run_case memfd-node $R node:lts-slim node -e "console.log(1)"
unset PODVM_MODE
echo "## static payload (zig cc -static hello), the only payload the memfd family takes"
S=$WORK/static; rm -rf "$S"; mkdir -p "$S/root/bin"
printf '#include <stdio.h>\nint main(void){puts("static-ok");return 0;}\n' >"$S/h.c"
if command -v zig >/dev/null && zig cc -target x86_64-linux-musl -static -o "$S/root/bin/hello" "$S/h.c" 2>"$S/zig.err"; then
  tar -cf "$S/static.tar" -C "$S/root" . && $PODVM rmi static-hello:t >/dev/null 2>&1; $PODVM import "$S/static.tar" static-hello:t 2>&1 | tail -1
  run_case static-default $R static-hello:t /bin/hello
  run_case static-default-unsafe $R --unsafe-host-paths static-hello:t /bin/hello
  PODVM_MODE=memfd run_case static-memfd $R static-hello:t /bin/hello
  PODVM_MODE=memfd run_case static-memfd-unsafe $R --unsafe-host-paths static-hello:t /bin/hello
else echo "no zig cc: static cases skipped"; cat "$S/zig.err" 2>/dev/null | head -3; fi
exit $FAIL
