#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The self-test of scripts/ghcr.py (T-553): `python3 -I scripts/ghcr.py --self-test`.

It builds the images of two fake binaries and of two fixture archives,
pushes them to a registry in memory, reads them back, and checks each
refusal. G is the module of ghcr.py.
"""

import contextlib
import gzip
import io
import json
import pathlib
import shutil
import tarfile


def fake_registry(g):
    """A registry in memory for the self-test: blobs, uploads, manifests."""
    import threading
    from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

    state = {"blobs": {}, "manifests": {}, "uploads": {}, "sent": 0}

    class H(BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass

        def reply(self, code, headers=None, body=b""):
            self.send_response(code)
            for k, v in (headers or {}).items():
                self.send_header(k, v)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(body)

        def body(self) -> bytes:
            return self.rfile.read(int(self.headers.get("Content-Length") or 0))

        def do_HEAD(self):
            self.reply(200 if self.path.rsplit("/", 1)[-1] in state["blobs"] else 404)

        def do_GET(self):
            ref = self.path.rsplit("/", 1)[-1]
            if "/manifests/" in self.path and ref in state["manifests"]:
                t, data = state["manifests"][ref]
                return self.reply(200, {"Content-Type": t, "Docker-Content-Digest": g.sha(data)}, data)
            self.reply(404)

        def do_POST(self):
            self.body()
            uid = str(len(state["uploads"]))
            state["uploads"][uid] = bytearray()
            self.reply(202, {"Location": f"/v2/u/blobs/uploads/{uid}"})

        def do_PATCH(self):
            state["uploads"][self.path.rsplit("/", 1)[-1]] += self.body()
            self.reply(202, {"Location": self.path})

        def do_PUT(self):
            if "/manifests/" in self.path:
                data = self.body()
                ref = self.path.rsplit("/", 1)[-1]
                if ref.startswith("sha256:") and ref != g.sha(data):
                    return self.reply(400)
                state["manifests"][ref] = state["manifests"][g.sha(data)] = (self.headers["Content-Type"], data)
                return self.reply(201, {"Docker-Content-Digest": g.sha(data)})
            path, _, q = self.path.partition("?")
            data = bytes(state["uploads"][path.rsplit("/", 1)[-1]])
            if g.sha(data) != q.split("digest=", 1)[-1].replace("%3A", ":"):
                return self.reply(400)
            state["blobs"][g.sha(data)] = data
            state["sent"] += 1
            self.reply(201)

    srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv, state


def fixture_archive(g, path: pathlib.Path, arch: str, seed: bytes, many: bool = False) -> str:
    """A runner image archive in the form of runner-image.yml; gives its manifest digest."""
    d = path.with_suffix(".d")
    lay = g.put(d, gzip.compress(g.layer("seed", b"\x7fELF" + seed), mtime=0))
    cfg = g.put(d, g.oci.canon({"architecture": arch, "os": "linux", "rootfs": {"type": "layers", "diff_ids": []}}))
    man = g.oci.canon({"schemaVersion": 2, "mediaType": g.oci.MANIFEST, "config": {"mediaType": g.oci.CONFIG, **cfg},
                     "layers": [{"mediaType": g.GZIP_LAYER, **lay}]})
    md = g.put(d, man)
    desc = {"mediaType": g.oci.MANIFEST, **md, "annotations": {g.REF: "runner:9.9.9"}}
    (d / "index.json").write_text(json.dumps({"schemaVersion": 2, "manifests": [desc] * (2 if many else 1)}))
    with tarfile.open(path, "w") as t:
        for f in sorted(d.rglob("*")):
            if f.is_file():
                t.add(f, f.relative_to(d).as_posix())
    return md["digest"]


def self_test(g) -> int:
    import tempfile

    bad = []

    def refused(why, fn, *args):
        try:
            fn(*args)
            bad.append(f"{why} is taken")
        except (ValueError, RuntimeError, KeyError):
            pass

    rev = "a" * 40
    with tempfile.TemporaryDirectory() as t:
        t = pathlib.Path(t)
        x, y = t / "podrunner-x86_64-unknown-linux-musl", t / "podrunner-aarch64-unknown-linux-musl"
        x.write_bytes(b"\x7fELF x86_64")
        y.write_bytes(b"\x7fELF aarch64")
        files = [("x86_64", x), ("aarch64", y)]
        d1 = g.binary("podrunner", "v9.9.9-beta.1", rev, files, t / "a")
        if g.binary("podrunner", "v9.9.9-beta.1", rev, files[::-1], t / "b") != d1:
            bad.append("the same files give two digests")
        tag, digest, data = g.read_layout(t / "a")
        ix = json.loads(data)
        if (tag, digest) != ("v9.9.9-beta.1", d1) or [e["platform"]["architecture"] for e in ix["manifests"]] \
                != ["amd64", "arm64"]:
            bad.append(f"the layout: {tag} {digest} {ix}")
        if ix["annotations"]["org.opencontainers.image.revision"] != rev:
            bad.append(f"the annotations of the index: {ix['annotations']}")
        rows = g.check_binary("podrunner", t / "a", files)
        if [r.split()[0] for r in rows] != ["linux/amd64", "linux/arm64"] or x.name not in rows[0]:
            bad.append(f"the rows of check-binary: {rows}")
        # The archive of one platform that `podvm load -i` takes.
        pd = g.podvm_archive(t / "a", "aarch64", "podrunner:test", t / "arm64.tar")
        with tarfile.open(t / "arm64.tar") as a:
            names = sorted(a.getnames())
            top = json.load(a.extractfile("index.json"))
        if pd != ix["manifests"][1]["digest"] or len(names) != 5 or "blobs/sha256/" + digest[7:] in names \
                or [(e["digest"], e["annotations"][g.REF]) for e in top["manifests"]] != [(pd, "podrunner:test")]:
            bad.append(f"the archive of podvm: {names} {top}")
        g.binary("podrunner", "v1", rev, [("x86_64", x)], t / "one")
        refused("an archive of a missing platform", g.podvm_archive, t / "one", "aarch64", "x:y", t / "n.tar")
        refused("a swapped platform", g.check_binary, "podrunner", t / "a", [("x86_64", y), ("aarch64", x)])
        refused("a missing platform", g.check_binary, "podrunner", t / "a", [("x86_64", x)])
        refused("another name", g.check_binary, "podvm", t / "a", files)
        refused("a file that is not ELF", g.binary, "podrunner", "v1", rev, [("x86_64", t / "a" / "index.json")],
                t / "c")
        amd64 = json.loads(g.path_of(t / "a", ix["manifests"][0]["digest"]).read_bytes())
        if g.path_of(t / "a", amd64["layers"][0]["digest"]).read_bytes()[4:8] != bytes(4):
            bad.append("the gzip of the layer has a time stamp")
        refused("two images of one arch", g.finish, t / "c", [ix["manifests"][0]] * 2, "v1", {})
        refused("a bad tag", g.binary, "podrunner", "v1:x", rev, files, t / "c")
        refused("a bad revision", g.binary, "podrunner", "v1", "HEAD", files, t / "c")
        refused("an arch twice", g.pairs, [f"x86_64={x}", f"x86_64={y}"])
        refused("another arch", g.pairs, [f"riscv64={x}"])
        # The runner image archives: their bytes as they are.
        ra, rb = t / "runner-x86_64.tar", t / "runner-aarch64.tar"
        da, db = fixture_archive(g, ra, "amd64", b"a"), fixture_archive(g, rb, "arm64", b"b")
        g.archives("v9.9.9-beta.1", rev, [("x86_64", ra), ("aarch64", rb)], t / "r")
        got = {e["platform"]["architecture"]: e["digest"] for e in json.loads(g.read_layout(t / "r")[2])["manifests"]}
        if got != {"amd64": da, "arm64": db}:
            bad.append(f"the digests of the archives: {got}, not {da} {db}")
        refused("an archive of another arch", g.archives, "v1", rev, [("x86_64", rb)], t / "s")
        fixture_archive(g, t / "two.tar", "amd64", b"c", many=True)
        refused("an archive with two manifests", g.archives, "v1", rev, [("x86_64", t / "two.tar")], t / "s")
        # A blob with other bytes than its name.
        fixture_archive(g, t / "evil.tar", "amd64", b"d")
        with tarfile.open(t / "evil.tar") as src, tarfile.open(t / "evil2.tar", "w") as dst:
            for m in src.getmembers():
                b = src.extractfile(m).read()
                if m.name.startswith("blobs/") and b[:2] == b"\x1f\x8b":
                    b = gzip.compress(b"x", mtime=0)
                    m.size = len(b)
                dst.addfile(m, io.BytesIO(b))
        refused("a blob with other bytes", g.archives, "v1", rev, [("x86_64", t / "evil2.tar")], t / "s")
        # The command line of each command that needs no registry.
        calls = [["binary", "--name", "podrunner", "--tag", "v1", "--revision", rev, "--out", str(t / "m1"),
                  f"x86_64={x}", f"aarch64={y}"],
                 ["check-binary", "--name", "podrunner", str(t / "m1"), f"x86_64={x}", f"aarch64={y}"],
                 ["podvm-archive", "--arch", "aarch64", "--ref", "podrunner:v1", str(t / "m1"), str(t / "m1.tar")],
                 ["archives", "--tag", "v1", "--revision", rev, "--out", str(t / "m2"), f"x86_64={ra}"]]
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            codes = [g.main(c) for c in calls] + [g.main(["podvm-archive", "--arch", "aarch64", str(t / "m1")])]
        if codes != [0, 0, 0, 0, 2]:
            bad.append(f"the exit codes of the command lines: {codes}")
        # The push and the read back.
        srv, state = fake_registry(g)
        reg = g.pb.Registry(f"http://127.0.0.1:{srv.server_port}", "made-up")
        if g.push(reg, "podutils/podrunner", t / "a") != d1 or state["sent"] != 4:
            bad.append(f"the push: {state['sent']} blobs")
        if g.push(reg, "podutils/podrunner", t / "a") != d1 or state["sent"] != 4:
            bad.append("a blob that the registry has is sent again")
        rows = g.verify(reg, "podutils/podrunner", t / "a")
        if [r.split()[0] for r in rows] != ["each", "linux/amd64", "linux/arm64"] or rows[0] != f"each {d1}":
            bad.append(f"the rows of verify: {rows}")
        g.push(reg, "podutils/podrunner-runner", t / "r")
        if g.verify(reg, "podutils/podrunner-runner", t / "r")[1] != f"linux/amd64 {da}":
            bad.append("the read back of the runner images")
        # The tag names other bytes; a blob is gone.
        state["manifests"]["v9.9.9-beta.1"] = (g.oci.INDEX, g.oci.canon({**ix, "annotations": {}}))
        refused("a tag of other bytes", g.verify, reg, "podutils/podrunner", t / "a")
        state["manifests"]["v9.9.9-beta.1"] = state["manifests"][d1]
        md = ix["manifests"][0]["digest"]
        keep = state["manifests"][md]
        state["manifests"][md] = (g.oci.MANIFEST, g.oci.canon({**amd64, "annotations": {}}))
        refused("a manifest of other bytes", g.verify, reg, "podutils/podrunner", t / "a")
        state["manifests"][md] = keep
        # A blob of the layout with other bytes than its name.
        shutil.copytree(t / "a", t / "a2")
        g.path_of(t / "a2", amd64["config"]["digest"]).write_bytes(b"{}")
        refused("a blob of the layout with other bytes", g.push, reg, "podutils/other", t / "a2")
        state["blobs"].pop(amd64["layers"][0]["digest"])
        refused("a missing blob", g.verify, reg, "podutils/podrunner", t / "a")
        srv.shutdown()
    for b in bad:
        print(f"self-test: {b}")
    if not bad:
        print("ghcr.py: self-test passes")
    return 1 if bad else 0
