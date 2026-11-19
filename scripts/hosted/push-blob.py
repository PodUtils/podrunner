#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""Push one blob to an OCI registry in chunks (T-542, T-555).

    REGISTRY_TOKEN=... python3 -I scripts/hosted/push-blob.py https://ghcr.io NAME FILE
    python3 -I scripts/hosted/push-blob.py --self-test

A layer of the snapshot can be 8 GB; ghcr.io ends a request after 10
minutes. So the upload is the chunked form of the distribution spec: POST
to open, one PATCH for each chunk with its Content-Range, PUT with the
digest to close. A blob that the registry has already (HEAD) is not sent
again. A failed chunk is sent again, up to three times, from the offset
that the registry gives. The token comes from the environment, never from
argv. It prints the digest.
"""

import hashlib
import http.client
import os
import sys
import urllib.parse

CHUNK = 256 << 20


def digest_of(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while b := f.read(1 << 20):
            h.update(b)
    return "sha256:" + h.hexdigest()


class Registry:
    def __init__(self, base: str, token: str | None):
        u = urllib.parse.urlsplit(base)
        self.https = u.scheme == "https"
        self.host = u.netloc
        self.token = token

    def request(self, method: str, url: str, body=None, headers=None) -> http.client.HTTPResponse:
        u = urllib.parse.urlsplit(url)
        host = u.netloc or self.host
        conn = (http.client.HTTPSConnection if self.https else http.client.HTTPConnection)(host, timeout=600)
        h = dict(headers or {})
        if self.token:
            h["Authorization"] = f"Bearer {self.token}"
        path = u.path + (f"?{u.query}" if u.query else "")
        conn.request(method, path, body=body, headers=h)
        r = conn.getresponse()
        r.read_body = r.read()
        conn.close()
        return r


def push(reg: Registry, name: str, path: str, chunk: int = CHUNK) -> str:
    digest = digest_of(path)
    if reg.request("HEAD", f"/v2/{name}/blobs/{digest}").status == 200:
        return digest
    r = reg.request("POST", f"/v2/{name}/blobs/uploads/", headers={"Content-Length": "0"})
    if r.status != 202:
        raise RuntimeError(f"open the upload: HTTP {r.status}")
    loc = r.getheader("Location")
    size = os.path.getsize(path)
    with open(path, "rb") as f:
        offset, tries = 0, 0
        while offset < size:
            f.seek(offset)
            data = f.read(min(chunk, size - offset))
            r = reg.request("PATCH", loc, body=data, headers={
                "Content-Type": "application/octet-stream",
                "Content-Range": f"{offset}-{offset + len(data) - 1}",
                "Content-Length": str(len(data)),
            })
            if r.status != 202:
                tries += 1
                if tries > 3:
                    raise RuntimeError(f"a chunk at {offset}: HTTP {r.status}")
                # Ask where the registry is, and go on from there.
                s = reg.request("GET", loc)
                rng = s.getheader("Range") or ""
                offset = int(rng.split("-")[1]) + 1 if "-" in rng else 0
                continue
            tries = 0
            loc = r.getheader("Location") or loc
            offset += len(data)
    sep = "&" if "?" in loc else "?"
    r = reg.request("PUT", f"{loc}{sep}digest={urllib.parse.quote(digest)}", headers={"Content-Length": "0"})
    if r.status != 201:
        raise RuntimeError(f"close the upload: HTTP {r.status}")
    return digest


def self_test() -> int:
    import tempfile
    import threading
    from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

    blobs: dict[str, bytes] = {}
    uploads: dict[str, bytearray] = {}
    seen: list[str] = []
    fail_once = {"done": False}

    class H(BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass

        def reply(self, code, headers=None):
            self.send_response(code)
            for k, v in (headers or {}).items():
                self.send_header(k, v)
            self.send_header("Content-Length", "0")
            self.end_headers()

        def body(self) -> bytes:
            return self.rfile.read(int(self.headers.get("Content-Length") or 0))

        def do_HEAD(self):
            self.reply(200 if self.path.rsplit("/", 1)[-1] in blobs else 404)

        def do_POST(self):
            self.body()
            uid = str(len(uploads))
            uploads[uid] = bytearray()
            seen.append("POST")
            self.reply(202, {"Location": f"/v2/x/blobs/uploads/{uid}"})

        def do_GET(self):
            uid = self.path.rsplit("/", 1)[-1]
            n = len(uploads[uid])
            self.reply(204, {"Range": f"0-{n - 1}" if n else "0-0", "Location": self.path})

        def do_PATCH(self):
            uid = self.path.rsplit("/", 1)[-1]
            data = self.body()
            start = int(self.headers["Content-Range"].split("-")[0])
            if start == 4 and not fail_once["done"]:
                fail_once["done"] = True
                return self.reply(500)
            if start != len(uploads[uid]):
                return self.reply(416)
            uploads[uid] += data
            seen.append(f"PATCH {start}")
            self.reply(202, {"Location": self.path})

        def do_PUT(self):
            path, _, q = self.path.partition("?")
            uid = path.rsplit("/", 1)[-1]
            want = urllib.parse.parse_qs(q)["digest"][0]
            got = "sha256:" + hashlib.sha256(uploads[uid]).hexdigest()
            if got != want:
                return self.reply(400)
            blobs[want] = bytes(uploads[uid])
            self.reply(201)

    srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    reg = Registry(f"http://127.0.0.1:{srv.server_port}", "made-up")
    bad = []
    with tempfile.TemporaryDirectory() as t:
        f = os.path.join(t, "layer")
        open(f, "wb").write(b"0123456789")
        d = push(reg, "x", f, chunk=4)
        if blobs.get(d) != b"0123456789":
            bad.append(f"the blob: {blobs}")
        if seen != ["POST", "PATCH 0", "PATCH 4", "PATCH 8"]:
            bad.append(f"the chunks, with one sent again: {seen}")
        seen.clear()
        if push(reg, "x", f, chunk=4) != d or seen:
            bad.append(f"a blob that the registry has is sent again: {seen}")
        # A wrong digest is refused by the registry.
        uploads["w"] = bytearray(b"abc")
        r = reg.request("PUT", "/v2/x/blobs/uploads/w?digest=sha256:" + "0" * 64, headers={"Content-Length": "0"})
        if r.status != 400:
            bad.append(f"a wrong digest: HTTP {r.status}")
    srv.shutdown()
    for b in bad:
        print(f"self-test: {b}")
    if not bad:
        print("push-blob.py: self-test passes")
    return 1 if bad else 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    if len(sys.argv) != 4:
        print(__doc__.strip().splitlines()[2], file=sys.stderr)
        return 2
    base, name, path = sys.argv[1:]
    print(push(Registry(base, os.environ.get("REGISTRY_TOKEN")), name, path))
    return 0


if __name__ == "__main__":
    sys.exit(main())
