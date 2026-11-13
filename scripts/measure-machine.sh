#!/bin/sh
# Measure the machine tier of podvm for the microvm kind (T-037, step 1):
# the runner image boots as a guest, a 9p volume takes a write back to the
# host, and the guest reaches a listener on the loopback of the host at
# the gateway 10.0.2.2 of the user-mode NIC.
#
#   sh scripts/measure-machine.sh PODVM STORE IMAGE WORKDIR
#
# STORE holds IMAGE (podvm load). It prints one line for each property:
# "machine NAME ok" or "machine NAME failed: why". The machine tier loads
# the image as an initramfs, which must fit in the guest memory twice
# (packed and unpacked): MACHINE_MEM sets it (default 4G).
set -u
podvm=$1
store=$2
image=$3
work=$4
mkdir -p "$work/vol"
chmod 0777 "$work/vol"
rm -f "$work/vol/written" "$work/vol/runner-wrote"
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
mem=${MACHINE_MEM:-4G}
out=$(timeout 300 "$podvm" run --rm --podvm-tier=machine --podvm-mem "$mem" --podvm-cpus 2 -v "$work/vol:/mnt/vol" "$image" /bin/sh -c "
  test -x /home/runner/run.sh && echo 'guest files ok'
  echo w > /mnt/vol/written && echo 'guest wrote'
  if setpriv --reuid=runner --regid=runner --init-groups -- sh -c 'echo r > /mnt/vol/runner-wrote'; then echo 'guest runner wrote'; else echo 'guest runner write failed'; fi
  ls -ld /mnt/vol; id runner
  stat -c 'guest stat %n %U:%G %a' /home/runner /home/runner/run.sh /home/runner/bin/Runner.Listener 2>&1
  setpriv --reuid=runner --regid=runner --init-groups -- /home/runner/run.sh --version 2>&1 | sed 's/^/guest runner-version /'; echo 'guest runner-version exit' \$?
  curl -s --max-time 20 http://10.0.2.2:$port/ || echo 'guest curl failed'
  uname -r
" 2>"$work/stderr")
code=$?
end=$(date +%s)
kill "$listener" 2>/dev/null

accel=$(grep -o 'kvm\|tcg' "$work/stderr" | head -1)
echo "machine boot exit=$code seconds=$((end - start)) accel=${accel:-unknown} mem=$mem"
case "$out" in *"guest files ok"*) echo "machine files ok" ;; *) echo "machine files failed: $(tail -c 300 "$work/stderr" | tr '\n' ' ')" ;; esac
if [ -f "$work/vol/written" ]; then echo "machine 9p ok"; else echo "machine 9p failed: no file on the host"; fi
if [ -f "$work/vol/runner-wrote" ]; then echo "machine 9p-runner ok"; else echo "machine 9p-runner failed: $(printf '%s\n' "$out" | grep -E 'runner|^d' | tr '\n' ' ')"; fi
case "$out" in *host-loopback-42*) echo "machine host-loopback ok" ;; *) echo "machine host-loopback failed" ;; esac
echo "machine kernel $(printf '%s\n' "$out" | tail -1)"
printf '%s\n' "$out" | sed 's/^/machine out /'

# T-037: a guest with no NIC and one channel (podvm --podvm-net none
# --podvm-channel). A unix socket of the host answers each connection with
# one line; the guest connects to 127.0.0.1:8080 twice, with curl and with
# the shell of the image, as root and as runner.
ch="$work/channel.sock"
rm -f "$ch" "$work/channel.log"
python3 -I - "$ch" "$work/channel.log" <<'PY' &
import socket, sys, threading
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.bind(sys.argv[1]); s.listen(8); s.settimeout(240)
log = open(sys.argv[2], "a", buffering=1)
def one(c):
    log.write("accept\n")
    c.settimeout(30)
    try:
        data = c.recv(4096)
        log.write("read %d\n" % len(data))
        c.sendall(b"HTTP/1.0 200 OK\r\nContent-Length: 11\r\n\r\nchannel-42\n")
    except Exception as e:
        log.write("error %s\n" % e)
    c.close()
while True:
    try:
        c, _ = s.accept()
    except Exception:
        break
    threading.Thread(target=one, args=(c,)).start()
PY
server=$!
i=0
while [ ! -S "$ch" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
start=$(date +%s)
out=$(timeout 300 "$podvm" run --rm --podvm-tier=machine --podvm-net none --podvm-channel "$ch:127.0.0.1:8080" \
  --podvm-mem "$mem" --podvm-cpus 2 "$image" /bin/sh -c "
  echo \"guest net \$(ls /sys/class/net | tr '\n' ' ')\"
  curl -s --max-time 20 http://127.0.0.1:8080/ || echo 'guest channel-curl failed'
  setpriv --reuid=runner --regid=runner --init-groups -- curl -s --max-time 20 -x http://127.0.0.1:8080 http://broker.podrunner.test/ || echo 'guest channel-proxy failed'
" 2>"$work/stderr-ch")
code=$?
end=$(date +%s)
kill "$server" 2>/dev/null
echo "machine channel exit=$code seconds=$((end - start))"
case "$out" in *"guest net lo "*) echo "machine no-nic ok" ;; *) echo "machine no-nic failed: $(printf '%s\n' "$out" | grep 'guest net')" ;; esac
n=$(printf '%s\n' "$out" | grep -c channel-42)
if [ "$n" -eq 2 ]; then echo "machine channel ok"; else echo "machine channel failed: $n of 2 answers"; fi
echo "machine channel host-side: $(tr '\n' ' ' < "$work/channel.log" 2>/dev/null)"
printf '%s\n' "$out" | sed 's/^/machine channel out /'
tail -c 600 "$work/stderr-ch" | sed 's/^/machine channel stderr /'
