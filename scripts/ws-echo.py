#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The WebSocket echo server of the user-path gate (T-506): the far end of
the attach of a sandbox. Standard library only.

  ws-echo.py --listen HOST:PORT
      serve; print ws://HOST:PORT/attach on the first line of stdout, and
      one line on stderr for each open, message and close
  ws-echo.py --self-test
      the server against scripts/ws-client.py: direct, through a stand-in
      CONNECT proxy that allows one port, and the faults that the client
      must find
"""

import argparse
import base64
import hashlib
import os
import pathlib
import socket
import subprocess
import sys
import tempfile
import threading
import time

GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
CLIENT = pathlib.Path(__file__).resolve().parent / "ws-client.py"


def accept_of(key: str) -> str:
    return base64.b64encode(hashlib.sha1((key + GUID).encode()).digest()).decode()


def log(out, text):
    out.write(f"ws-echo: {text}\n")
    out.flush()


def exact(c, n):
    data = b""
    while len(data) < n:
        part = c.recv(n - len(data))
        if not part:
            raise EOFError
        data += part
    return data


def frame(op, data):
    """A frame of the server: not masked."""
    n = len(data)
    size = bytes([n]) if n < 126 else bytes([126]) + n.to_bytes(2, "big")
    return bytes([0x80 | op]) + size + data


def session(c, peer, out, plain=False):
    """One connection: the upgrade, then each text frame back, until the
    close. With PLAIN, answer 200 with no upgrade (a fault for the test)."""
    t0, count = time.monotonic(), 0
    try:
        head = b""
        while b"\r\n\r\n" not in head:
            part = c.recv(4096)
            if not part or len(head) > 16384:
                return
            head += part
        lines = head.decode("latin-1").split("\r\n")
        fields = {k.strip().lower(): v.strip() for k, _, v in (x.partition(":") for x in lines[1:] if x)}
        key = fields.get("sec-websocket-key", "")
        if plain or fields.get("upgrade", "").lower() != "websocket" or not key:
            c.sendall(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            log(out, f"no upgrade from {peer}: {lines[0]}")
            return
        accept = accept_of(key)
        c.sendall(f"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                  f"Sec-WebSocket-Accept: {accept}\r\n\r\n".encode())
        log(out, f"open {peer} {lines[0]}")
        while True:
            b0, b1 = exact(c, 2)
            n = b1 & 0x7F
            if n >= 126:
                n = int.from_bytes(exact(c, 2 if n == 126 else 8), "big")
            mask = exact(c, 4) if b1 & 0x80 else b"\0\0\0\0"
            data = bytes(b ^ mask[i % 4] for i, b in enumerate(exact(c, n)))
            op = b0 & 0x0F
            if op == 8:
                c.sendall(frame(8, data[:2]))
                break
            if op == 9:
                c.sendall(frame(10, data))
            elif op in (1, 2):
                count += 1
                log(out, f"message {count} from {peer}: {len(data)} bytes")
                c.sendall(frame(op, data))
        log(out, f"close {peer} after {time.monotonic() - t0:.1f} s, {count} messages")
    except (EOFError, OSError) as e:
        log(out, f"lost {peer} after {time.monotonic() - t0:.1f} s, {count} messages: {e or 'end of stream'}")
    finally:
        c.close()


def serve(sock, out, plain=False):
    while True:
        c, peer = sock.accept()
        threading.Thread(target=session, args=(c, f"{peer[0]}:{peer[1]}", out, plain), daemon=True).start()


def listen(where):
    host, _, port = where.rpartition(":")
    s = socket.create_server((host, int(port)))
    return s, f"ws://{host}:{s.getsockname()[1]}/attach"


# The self-test.
def pipe(a, b):
    try:
        while data := a.recv(65536):
            b.sendall(data)
    except OSError:
        pass
    for s in (a, b):
        try:
            s.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass


def proxy(sock, allowed, deny_status):
    """A stand-in of the egress module: CONNECT to port ALLOWED only; each
    other port gets DENY_STATUS."""
    while True:
        c, _ = sock.accept()
        head = b""
        while b"\r\n\r\n" not in head:
            head += c.recv(1)
        target = head.split(b" ")[1].decode()
        port = int(target.rpartition(":")[2])
        if port != allowed:
            c.sendall(f"HTTP/1.1 {deny_status} No\r\n\r\n".encode())
            c.close()
            continue
        far = socket.create_connection(("127.0.0.1", port))
        c.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
        threading.Thread(target=pipe, args=(c, far), daemon=True).start()
        threading.Thread(target=pipe, args=(far, c), daemon=True).start()


def self_test() -> int:
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="ws-echo-"))
    logs = {name: (tmp / f"{name}.log").open("w+") for name in ("good", "plain")}
    good, good_url = listen("127.0.0.1:0")
    plain, plain_url = listen("127.0.0.1:0")
    threading.Thread(target=serve, args=(good, logs["good"]), daemon=True).start()
    threading.Thread(target=serve, args=(plain, logs["plain"], True), daemon=True).start()
    port = int(good_url.split(":")[2].split("/")[0])
    proxies = {}
    for name, status in (("broker", 403), ("open", 200)):
        p = socket.create_server(("127.0.0.1", 0))
        threading.Thread(target=proxy, args=(p, port, status), daemon=True).start()
        proxies[name] = f"http://127.0.0.1:{p.getsockname()[1]}"
    env = {k: v for k, v in os.environ.items() if k.lower() not in ("http_proxy", "https_proxy")}
    cases = [
        ("direct", good_url, {}, 0, "echo ok, held 1.0 s, direct"),
        ("through the CONNECT of http_proxy", good_url, {"http_proxy": proxies["broker"]}, 0, "through the proxy"),
        ("a server with no upgrade", plain_url, {}, 1, "no upgrade"),
        ("a proxy that opens a port off the list", good_url, {"http_proxy": proxies["open"]}, 1, "not 403"),
        ("a proxy that refuses the port of the URL", plain_url, {"http_proxy": proxies["broker"]}, 1, "answered 403"),
    ]
    # The sample of RFC 6455, section 1.3: the server and the client
    # must not share a wrong GUID.
    ok = accept_of("dGhlIHNhbXBsZSBub25jZQ==") == "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=" and GUID in CLIENT.read_text()
    bad = not ok
    print(f"{'ok' if ok else 'FAIL'} the accept of the sample of RFC 6455, and the client has the same GUID")
    text = CLIENT.read_text()
    for name, url, extra, want_rc, want_text in cases:
        # As the sandbox command runs it: the text of the client to -c.
        p = subprocess.run([sys.executable, "-I", "-c", text, url, "1"], env={**env, **extra}, capture_output=True,
                           text=True, timeout=60)
        ok = p.returncode == want_rc and want_text in p.stdout + p.stderr
        bad += not ok
        print(f"{'ok' if ok else 'FAIL'} {name}: exit {p.returncode}: {(p.stdout + p.stderr).strip()}")
    logs["good"].seek(0)
    seen = logs["good"].read()
    want = ["open 127.0.0.1:", "message 2 from", "close 127.0.0.1:"]
    ok = all(w in seen for w in want) and seen.count("close ") >= 3
    bad += not ok
    print(f"{'ok' if ok else 'FAIL'} the log of the server has each open, message and close")
    if not ok:
        print(seen)
    print(f"ws-echo: {len(cases) + 2 - bad} of {len(cases) + 2} self-test cases passed")
    return 1 if bad else 0


def main() -> int:
    a = argparse.ArgumentParser(description="The WebSocket echo server of the user-path gate (T-506).")
    a.add_argument("--listen", metavar="HOST:PORT")
    a.add_argument("--self-test", action="store_true")
    a = a.parse_args()
    if a.self_test:
        return self_test()
    if not a.listen:
        print("ws-echo: give --listen HOST:PORT or --self-test", file=sys.stderr)
        return 2
    sock, url = listen(a.listen)
    print(url, flush=True)
    serve(sock, sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
