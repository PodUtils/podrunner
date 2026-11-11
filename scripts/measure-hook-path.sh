#!/bin/sh
# Measure the facts U21 of the hook path (T-034) on the podvm rung of this
# host. Run it as root (the namespace rung) and as a uid with no user
# namespace (the supervise rung). It needs python3 and curl on the host.
#
#   sh scripts/measure-hook-path.sh PODVM WORKDIR
#
# It prints one line for each fact: "FACT yes" or "FACT no: why".
#
# * curl-runs: curl (the relay needs it) runs in a unit.
# * bash-runs: bash runs in a unit.
# * volume-socket: a unit reaches a host unix socket through a volume
#   (`-v DIR:/ctl`, then `/ctl/u.ctl`).
# * host-path-socket: a unit reaches a host unix socket by its host path.
# * rootfs-socket: a unit reaches a unix socket that the host binds in the
#   rootfs of the container (`RootfsPath` of `podvm inspect`).
set -u
podvm=$1
work=$2
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$work/ctl"
export PODVM_STORE="$work/store"
"$podvm" probe 2>&1 | sed -n '2s/^/rung: /p'

python3 -I "$here/stand-in-image.py" "$work/image.tar" "$(command -v curl)" "$(command -v bash)" > /dev/null
"$podvm" load -i "$work/image.tar" > /dev/null 2>&1 || { echo "load no"; exit 1; }
image="glibc-mini:1"

# A server on a unix socket in DIR: one line for each request. It binds a
# relative name, so a long DIR passes the limit of sun_path (108 bytes).
serve() {
  python3 -I -c '
import os, socket, sys
os.chdir(sys.argv[1])
if os.path.exists("u.ctl"):
    os.unlink("u.ctl")
s = socket.socket(socket.AF_UNIX); s.bind("u.ctl"); s.listen(4)
print("listening", flush=True)
while True:
    c, _ = s.accept()
    c.recv(65536)
    c.sendall(b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\npodrunner-ctl")
    c.close()
' "$1" > "$work/server.log" 2>&1 &
  server=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do grep -q listening "$work/server.log" && break; sleep 1; done
}
server=
trap '[ -n "$server" ] && kill "$server" 2> /dev/null' EXIT

fact() {
  case "$2" in "$3") echo "$1 yes" ;; *) echo "$1 no: $2" ;; esac
}
curl_ctl="/bin/curl -sS --unix-socket"

fact curl-runs "$("$podvm" run --rm "$image" /bin/curl --version 2>&1 | grep -m1 -o '^curl')" curl
fact bash-runs "$("$podvm" run --rm "$image" /bin/bash -c 'echo $((6*7))' 2>&1 | tail -1)" 42

serve "$work/ctl"
fact volume-socket "$("$podvm" run --rm -v "$work/ctl:/ctl" "$image" $curl_ctl /ctl/u.ctl http://unit/x 2>&1 | tail -1)" podrunner-ctl
fact host-path-socket "$("$podvm" run --rm "$image" $curl_ctl "$work/ctl/u.ctl" http://unit/x 2>&1 | tail -1)" podrunner-ctl
kill "$server"

"$podvm" rm -f m21 > /dev/null 2>&1
"$podvm" run -d --name m21 "$image" /bin/sleep 120 > /dev/null 2>&1
root=$("$podvm" inspect m21 2>/dev/null | python3 -I -c 'import json, sys; print(json.load(sys.stdin)[0]["RootfsPath"])')
serve "$root/home"
fact rootfs-socket "$("$podvm" exec m21 $curl_ctl /home/u.ctl http://unit/x 2>&1 | tail -1)" podrunner-ctl
kill "$server"
rm -f "$root/home/u.ctl"
"$podvm" rm -f m21 > /dev/null 2>&1
