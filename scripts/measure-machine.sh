#!/bin/sh
# Measure the machine tier of podvm for the microvm kind (T-037, step 1):
# the runner image boots as a guest, a 9p volume takes a write back to the
# host, and the guest reaches a listener on the loopback of the host at
# the gateway 10.0.2.2 of the user-mode NIC.
#
#   sh scripts/measure-machine.sh PODVM STORE IMAGE WORKDIR
#
# STORE holds IMAGE (podvm load). It prints one line for each property:
# "machine NAME ok" or "machine NAME failed: why".
set -u
podvm=$1
store=$2
image=$3
work=$4
mkdir -p "$work/vol"
rm -f "$work/vol/written"
export PODVM_STORE="$store"

# A listener on the loopback of the host. It answers one GET.
port_file="$work/port"
rm -f "$port_file"
python3 -I - "$port_file" <<'PY' &
import http.server, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200); self.end_headers(); self.wfile.write(b"host-loopback-42\n")
    def log_message(self, *a): pass
s = http.server.HTTPServer(("127.0.0.1", 0), H)
open(sys.argv[1], "w").write(str(s.server_address[1]))
s.timeout = 240
s.handle_request()
PY
listener=$!
i=0
while [ ! -s "$port_file" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
port=$(cat "$port_file")

start=$(date +%s)
out=$(timeout 300 "$podvm" run --rm --podvm-tier=machine -v "$work/vol:/mnt/vol" "$image" /bin/sh -c "
  test -x /home/runner/run.sh && echo 'guest files ok'
  echo w > /mnt/vol/written && echo 'guest wrote'
  curl -s --max-time 20 http://10.0.2.2:$port/ || echo 'guest curl failed'
  uname -r
" 2>"$work/stderr")
code=$?
end=$(date +%s)
kill "$listener" 2>/dev/null

accel=$(grep -o 'kvm\|tcg' "$work/stderr" | head -1)
echo "machine boot exit=$code seconds=$((end - start)) accel=${accel:-unknown}"
case "$out" in *"guest files ok"*) echo "machine files ok" ;; *) echo "machine files failed: $(tail -c 300 "$work/stderr" | tr '\n' ' ')" ;; esac
if [ -f "$work/vol/written" ]; then echo "machine 9p ok"; else echo "machine 9p failed: no file on the host"; fi
case "$out" in *host-loopback-42*) echo "machine host-loopback ok" ;; *) echo "machine host-loopback failed" ;; esac
echo "machine kernel $(printf '%s\n' "$out" | tail -1)"
