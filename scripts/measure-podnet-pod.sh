#!/bin/sh
# Measure podnet pod around a podvm unit (T-065): `podnet pod --backend
# seccomp -x http://unix:SOCK -- podvm run -d`, with a CONNECT stand-in on
# SOCK that tunnels each target to a loopback server. The payload connects
# to 10.255.0.1, which no host serves, so an answer comes only through the
# stand-in. Run it as a uid with no user namespace.
#
#   sh scripts/measure-podnet-pod.sh PODNET PODVM WORKDIR
#
# It prints one line for each check: "CHECK ok" or "CHECK no: why".
set -u
podnet=$1
podvm=$2
work=$3
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$work"
export PODVM_STORE="$work/store"
sock="$work/egress.sock"
log="$work/connect.log"
: > "$log"
say() {
  if [ "$2" -eq 0 ]; then echo "$1 ok"; else echo "$1 no: $3"; fi
}

"$podnet" --version | head -1
"$podnet" pod doctor --json 2>/dev/null | python3 -I -c 'import json,sys; d=json.load(sys.stdin); print("hold:", json.dumps(d.get("hold", d))[:300])' || echo "pod doctor: no answer"
"$podvm" probe 2>&1 | sed -n '2s/^/rung: /p'

# A loopback server with a known body, and the stand-in in front of it.
port=$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
mkdir -p "$work/www" && echo podrunner-through-the-hold > "$work/www/index.html"
(cd "$work/www" && exec python3 -I -m http.server "$port" --bind 127.0.0.1 > /dev/null 2>&1) &
www=$!
python3 -I "$here/connect-stand-in.py" "$sock" "$log" "$port" &
stand=$!
trap 'kill $www $stand 2> /dev/null; "$podvm" rm -f hold > /dev/null 2>&1' EXIT
sleep 1

python3 -I "$here/stand-in-image.py" "$work/image.tar" "$(command -v curl)" > /dev/null
"$podvm" load -i "$work/image.tar" > /dev/null 2>&1 || { echo "load no"; exit 1; }
image="glibc-mini:1"

# The unit: a connect at once, then a long sleep for exec.
"$podvm" rm -f hold > /dev/null 2>&1
start=$(date +%s)
"$podnet" pod --backend seccomp -x "http://unix:$sock" -- \
  "$podvm" run -d --name hold "$image" /bin/sh -c \
  "curl -s --max-time 10 http://10.255.0.1:$port/ > /tmp/run.txt 2>&1; echo \$? >> /tmp/run.txt; sleep 6; curl -s --max-time 10 http://10.255.0.1:$port/ > /tmp/late.txt 2>&1; echo \$? >> /tmp/late.txt; exec sleep 120" \
  > "$work/out" 2>&1
rc=$?
say run-d-under-podnet-pod "$rc" "exit $rc: $(tail -1 "$work/out")"
echo "podnet pod returned after $(( $(date +%s) - start )) s"
sleep 12

"$podvm" exec hold /bin/cat /tmp/run.txt > "$work/out" 2>&1
grep -q podrunner-through-the-hold "$work/out"
say connect-of-the-payload-reaches-the-socket $? "$(tr '\n' ' ' < "$work/out" | cut -c1-200)"
grep -q "CONNECT 10.255.0.1:$port" "$log"
say the-socket-logs-the-target $? "log: $(tr '\n' ' ' < "$log" | cut -c1-200)"

# A connect of the payload 6 s later, after podnet pod returned.
"$podvm" exec hold /bin/cat /tmp/late.txt > "$work/out" 2>&1
grep -q podrunner-through-the-hold "$work/out"
say late-connect-of-the-payload $? "$(tr '\n' ' ' < "$work/out" | cut -c1-200)"

# A step through exec, after podnet pod returned.
"$podvm" exec hold /bin/sh -c "curl -s --max-time 10 http://10.255.0.1:$port/" > "$work/out" 2>&1
grep -q podrunner-through-the-hold "$work/out"
say connect-of-an-exec-step $? "$(tail -1 "$work/out" | cut -c1-200)"

# A step through exec under a podnet pod of its own.
"$podnet" pod --backend seccomp -x "http://unix:$sock" -- \
  "$podvm" exec hold /bin/sh -c "curl -s --max-time 10 http://10.255.0.1:$port/" > "$work/out" 2>&1
grep -q podrunner-through-the-hold "$work/out"
say connect-of-an-exec-step-under-podnet-pod $? "$(tail -1 "$work/out" | cut -c1-200)"

# A direct connect to a loopback port of the host (advisory: it passes).
"$podvm" exec hold /bin/sh -c "curl -s --max-time 5 http://127.0.0.1:$port/" > "$work/out" 2>&1
grep -q podrunner-through-the-hold "$work/out"
say direct-loopback-connect "$?" "$(tail -1 "$work/out" | cut -c1-200)"

"$podvm" inspect hold > "$work/out" 2>&1
say inspect $? "$(tail -1 "$work/out")"
"$podvm" rm -f hold > "$work/out" 2>&1
say rm-f $? "$(tail -1 "$work/out")"
echo "connect log: $(wc -l < "$log") lines"
