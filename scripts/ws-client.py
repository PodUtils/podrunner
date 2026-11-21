# SPDX-License-Identifier: 0BSD
# The attach client of a sandbox in the user-path gate (T-506): the gate
# gives this text to `python3 -I -c` as the command of a sandbox offer,
# so it uses the standard library only and reads no other file.
#
#   python3 -I scripts/ws-client.py ws://HOST:PORT/PATH HOLD_S
#
# It opens a WebSocket to the URL (through the CONNECT of `http_proxy`
# when it is set: the egress module of the unit), sends a line, reads
# the echo, holds the connection for HOLD_S seconds, sends a second line,
# reads its echo, and closes. Through a proxy it also requires a 403 for
# a port that the allow list of the offer does not name. Exit 0 when each
# step passes, else 1 with the cause on stderr.
import base64, hashlib, os, socket, sys, time

url, hold = sys.argv[1], float(sys.argv[2])
if not url.startswith("ws://"):
    sys.exit(f"ws-client: {url!r} is not a ws:// URL")
where, _, path = url[5:].partition("/")
host, _, port = where.rpartition(":")
port, path = int(port), "/" + path
proxy = os.environ.get("http_proxy") or os.environ.get("HTTP_PROXY") or ""


def fail(why):
    print(f"ws-client: {why}", file=sys.stderr, flush=True)
    sys.exit(1)


def exact(s, n):
    data = b""
    while len(data) < n:
        part = s.recv(n - len(data))
        if not part:
            fail(f"the connection ended after {len(data)} of {n} bytes")
        data += part
    return data


def head(s):
    # One byte at a time: the bytes after the head are frames.
    data = b""
    while not data.endswith(b"\r\n\r\n"):
        if len(data) > 16384:
            fail("a head of more than 16 KiB")
        data += exact(s, 1)
    lines = data.decode("latin-1").split("\r\n")
    fields = {k.strip().lower(): v.strip() for k, _, v in (x.partition(":") for x in lines[1:] if x)}
    return lines[0], fields


def dial(p):
    """A socket to HOST:P and the status of the CONNECT (200 with no proxy)."""
    if not proxy:
        return socket.create_connection((host, p), timeout=30), 200
    pa, _, pp = proxy.split("://", 1)[-1].split("/", 1)[0].rpartition(":")
    s = socket.create_connection((pa, int(pp)), timeout=30)
    s.sendall(f"CONNECT {host}:{p} HTTP/1.1\r\nHost: {host}:{p}\r\n\r\n".encode())
    line, _ = head(s)
    return s, int(line.split(" ")[1])


def send(s, op, data):
    mask, n = os.urandom(4), len(data)
    size = bytes([0x80 | n]) if n < 126 else bytes([0x80 | 126]) + n.to_bytes(2, "big")
    s.sendall(bytes([0x80 | op]) + size + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))


def recv(s):
    b0, b1 = exact(s, 2)
    n = b1 & 0x7F
    if n >= 126:
        n = int.from_bytes(exact(s, 2 if n == 126 else 8), "big")
    if b1 & 0x80:
        fail("the server masked a frame")
    return b0 & 0x0F, exact(s, n)


def echo(s, text):
    send(s, 1, text.encode())
    op, data = recv(s)
    if op != 1 or data != text.encode():
        fail(f"the echo of {text!r} is opcode {op} with {data[:80]!r}")


s, status = dial(port)
if status != 200:
    fail(f"the proxy answered {status} to CONNECT {host}:{port}")
key = base64.b64encode(os.urandom(16)).decode()
s.sendall(
    f"GET {path} HTTP/1.1\r\nHost: {where}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
    f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n".encode()
)
line, fields = head(s)
want = base64.b64encode(hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest()).decode()
if line.split(" ")[1:2] != ["101"] or fields.get("sec-websocket-accept") != want:
    fail(f"no upgrade: {line!r}")
nonce = base64.b16encode(os.urandom(4)).decode()
echo(s, f"podrunner-sandbox-ws {nonce} open")
t0 = time.monotonic()
time.sleep(hold)
echo(s, f"podrunner-sandbox-ws {nonce} after {hold:g} s")
send(s, 8, (1000).to_bytes(2, "big"))
op, _ = recv(s)
if op != 8:
    fail(f"the answer to the close is opcode {op}")
s.close()
held = time.monotonic() - t0
if proxy:
    # The allow list of the offer names one port: the egress module
    # refuses each other port of the host.
    other = port + 1 if port < 65535 else port - 1
    d, status = dial(other)
    d.close()
    if status != 403:
        fail(f"the proxy answered {status}, not 403, to CONNECT {host}:{other} (not on the allow list)")
print(f"ws-client: echo ok, held {held:.1f} s, {'through the proxy' if proxy else 'direct'}", flush=True)
