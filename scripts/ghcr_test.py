#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The self-test of scripts/ghcr.py (T-553): `python3 -I scripts/ghcr.py --self-test`.

It builds the images of two fake binaries and of two fixture archives,
pushes them to a registry in memory, reads them back, probes the push of
a login, and checks each refusal by its type and its text. G is the
module of ghcr.py.
"""

import base64
import contextlib
import gzip
import io
import json
import pathlib
import shutil
import tarfile
import urllib.parse


def fake_registry(g):
    """A registry in memory for the self-test: blobs, uploads, manifests."""
    import threading
    from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

    # writers: the logins that get a token of push; scopes: the scope of
    # each token that /token gave (a made-up token of another test has
    # none and may push); cancel: DELETE of an upload works.
    state = {"blobs": {}, "manifests": {}, "uploads": {}, "sent": 0, "writers": {"made-up-writer"}, "scopes": {},
             "cancel": True}

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

        def bearer(self) -> str:
            return (self.headers.get("Authorization") or "").removeprefix("Bearer ")

        def do_GET(self):
            if self.path.startswith("/token?"):
                q = urllib.parse.parse_qs(self.path.partition("?")[2])
                auth = self.headers.get("Authorization") or ""
                login = base64.b64decode(auth.removeprefix("Basic ")).decode().partition(":")[2] if auth else ""
                if q.get("service") != ["127.0.0.1"] or not login.startswith("made-up"):
                    # As ghcr.io (measured): a login that cannot push gets 403.
                    return self.reply(403, body=b'{"errors":[{"code":"DENIED","message":'
                                                b'"requested access to the resource is denied"}]}')
                if login == "made-up-empty":
                    return self.reply(200, body=b"{}")
                # Another registry gives a token of pull only to a login that
                # cannot push: then the POST is refused.
                repo, _, want = q["scope"][0].removeprefix("repository:").rpartition(":")
                tok = f"t{len(state['scopes'])}"
                state["scopes"][tok] = (repo, want if login in state["writers"] else "pull")
                return self.reply(200, body=json.dumps({"token": tok}).encode())
            ref = self.path.rsplit("/", 1)[-1]
            if "/manifests/" in self.path and ref in state["manifests"]:
                t, data = state["manifests"][ref]
                return self.reply(200, {"Content-Type": t, "Docker-Content-Digest": g.sha(data)}, data)
            self.reply(404)

        def do_POST(self):
            self.body()
            repo = self.path.removeprefix("/v2/").partition("/blobs/")[0]
            scope = state["scopes"].get(self.bearer())
            if scope and (scope[0] != repo or "push" not in scope[1].split(",")):
                return self.reply(403, body=b'{"errors":[{"code":"DENIED",'
                                            b'"message":"permission_denied: write_package"}]}')
            uid = str(len(state["uploads"]))
            state["uploads"][uid] = bytearray()
            self.reply(202, {"Location": f"/v2/u/blobs/uploads/{uid}"})

        def do_DELETE(self):
            uid = self.path.rsplit("/", 1)[-1]
            if not state["cancel"]:
                return self.reply(405)
            self.reply(204 if state["uploads"].pop(uid, None) is not None else 404)

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

    def refused(why, exc, text, fn, *args):
        """FN(*ARGS) must raise EXC with TEXT in its message: a typo in
        the test (a KeyError, a TypeError) is no refusal."""
        try:
            fn(*args)
        except Exception as e:  # each other error is a fault of the test
            if type(e) is not exc or text not in str(e):
                bad.append(f"{why}: {type(e).__name__}: {e}, not {exc.__name__} with {text!r}")
            return
        bad.append(f"{why} is taken")

    # The check of the checker: an error of another type, or of another
    # text, is not a refusal.
    refused("a typo", ValueError, "x", lambda: {}["typo"])
    refused("another text", ValueError, "the text", lambda: int("x"))
    if len(bad) != 2 or not bad[0].startswith("a typo: KeyError") or not bad[1].startswith("another text: ValueError"):
        print(f"self-test: refused takes a wrong refusal: {bad}")
        return 1
    bad.clear()

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
        refused("an archive of a missing platform", ValueError, "no image of aarch64",
                g.podvm_archive, t / "one", "aarch64", "x:y", t / "n.tar")
        refused("a swapped platform", ValueError, "/usr/local/bin/podrunner is not podrunner-aarch64",
                g.check_binary, "podrunner", t / "a", [("x86_64", y), ("aarch64", x)])
        refused("a missing platform", ValueError, "the images are ['amd64', 'arm64'], not ['amd64']",
                g.check_binary, "podrunner", t / "a", [("x86_64", x)])
        refused("another name", ValueError, "the config is amd64 ['/usr/local/bin/podrunner']",
                g.check_binary, "podvm", t / "a", files)
        refused("a file that is not ELF", ValueError, "index.json: not an ELF file",
                g.binary, "podrunner", "v1", rev, [("x86_64", t / "a" / "index.json")], t / "c")
        amd64 = json.loads(g.path_of(t / "a", ix["manifests"][0]["digest"]).read_bytes())
        if g.path_of(t / "a", amd64["layers"][0]["digest"]).read_bytes()[4:8] != bytes(4):
            bad.append("the gzip of the layer has a time stamp")
        refused("two images of one arch", ValueError, "two images of one arch: ['amd64', 'amd64']",
                g.finish, t / "c", [ix["manifests"][0]] * 2, "v1", {})
        refused("a bad tag", ValueError, "'v1:x' is not a tag of a registry",
                g.binary, "podrunner", "v1:x", rev, files, t / "c")
        refused("a bad revision", ValueError, "'HEAD' is not the SHA of a commit",
                g.binary, "podrunner", "v1", "HEAD", files, t / "c")
        refused("an arch twice", ValueError, "an arch twice", g.pairs, [f"x86_64={x}", f"x86_64={y}"])
        refused("another arch", ValueError, "is not ARCH=FILE with ARCH in", g.pairs, [f"riscv64={x}"])
        # The runner image archives: their bytes as they are.
        ra, rb = t / "runner-x86_64.tar", t / "runner-aarch64.tar"
        da, db = fixture_archive(g, ra, "amd64", b"a"), fixture_archive(g, rb, "arm64", b"b")
        g.archives("v9.9.9-beta.1", rev, [("x86_64", ra), ("aarch64", rb)], t / "r")
        got = {e["platform"]["architecture"]: e["digest"] for e in json.loads(g.read_layout(t / "r")[2])["manifests"]}
        if got != {"amd64": da, "arm64": db}:
            bad.append(f"the digests of the archives: {got}, not {da} {db}")
        refused("an archive of another arch", ValueError, "the image is linux/arm64, not linux/amd64",
                g.archives, "v1", rev, [("x86_64", rb)], t / "s")
        fixture_archive(g, t / "two.tar", "amd64", b"c", many=True)
        refused("an archive with two manifests", ValueError, "2 manifests in index.json, not one",
                g.archives, "v1", rev, [("x86_64", t / "two.tar")], t / "s")
        # A blob with other bytes than its name.
        fixture_archive(g, t / "evil.tar", "amd64", b"d")
        with tarfile.open(t / "evil.tar") as src, tarfile.open(t / "evil2.tar", "w") as dst:
            for m in src.getmembers():
                b = src.extractfile(m).read()
                if m.name.startswith("blobs/") and b[:2] == b"\x1f\x8b":
                    b = gzip.compress(b"x", mtime=0)
                    m.size = len(b)
                dst.addfile(m, io.BytesIO(b))
        refused("a blob with other bytes", ValueError, "has other bytes",
                g.archives, "v1", rev, [("x86_64", t / "evil2.tar")], t / "s")
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
        refused("a tag of other bytes", RuntimeError, "podutils/podrunner:v9.9.9-beta.1 is sha256:",
                g.verify, reg, "podutils/podrunner", t / "a")
        state["manifests"]["v9.9.9-beta.1"] = state["manifests"][d1]
        md = ix["manifests"][0]["digest"]
        keep = state["manifests"][md]
        state["manifests"][md] = (g.oci.MANIFEST, g.oci.canon({**amd64, "annotations": {}}))
        refused("a manifest of other bytes", RuntimeError, ": the registry gives sha256:",
                g.verify, reg, "podutils/podrunner", t / "a")
        state["manifests"][md] = keep
        # A blob of the layout with other bytes than its name.
        shutil.copytree(t / "a", t / "a2")
        g.path_of(t / "a2", amd64["config"]["digest"]).write_bytes(b"{}")
        refused("a blob of the layout with other bytes", RuntimeError, f"the blob {amd64['config']['digest']} went up",
                g.push, reg, "podutils/other", t / "a2")
        state["blobs"].pop(amd64["layers"][0]["digest"])
        refused("a missing blob", RuntimeError, f"the blob {amd64['layers'][0]['digest']}: HTTP 404",
                g.verify, reg, "podutils/podrunner", t / "a")
        # T-553: the proof of the push before the tag. A login that can push
        # opens an upload and cancels it: nothing stays in the registry.
        base = f"http://127.0.0.1:{srv.server_port}"
        before = (dict(state["blobs"]), dict(state["manifests"]))
        state["uploads"].clear()
        for repo in ("podutils/podrunner", "podutils/podrunner-runner"):
            got = g.probe_push(base, repo, "made-up-writer")
            if got != f"{repo}: an upload opens and is cancelled: this login can push":
                bad.append(f"the probe of {repo}: {got}")
        if state["uploads"] or (state["blobs"], state["manifests"]) != before:
            bad.append(f"the probe left a change: {len(state['uploads'])} uploads")
        refused("a login that cannot push", RuntimeError,
                "podutils/podrunner: no upload opens (HTTP 403): this login cannot push to it "
                "DENIED: permission_denied: write_package", g.probe_push, base, "podutils/podrunner", "made-up-reader")
        refused("a wrong login", RuntimeError,
                "podutils/podrunner: the registry gives no token of pull,push (HTTP 403) DENIED: requested access",
                g.probe_push, base, "podutils/podrunner", "wrong")
        refused("an answer with no token", RuntimeError, "the answer of the token service has no token",
                g.probe_push, base, "podutils/podrunner", "made-up-empty")
        # A registry with no cancel: the upload expires; the probe passes.
        state["cancel"] = False
        if "the cancel gives HTTP 405" not in g.probe_push(base, "podutils/podrunner", "made-up-writer"):
            bad.append("a registry with no cancel fails the probe")
        srv.shutdown()
        # The layouts of a release: the same files give the same lines.
        dist, imgs = t / "dist", t / "imgs"
        dist.mkdir()
        imgs.mkdir()
        for a, f in files:
            shutil.copy(f, dist / f.name)
        shutil.copy(ra, imgs / ra.name)
        shutil.copy(rb, imgs / rb.name)
        shutil.copy(ra, imgs / "runner-full-x86_64.tar")
        with contextlib.redirect_stderr(io.StringIO()):
            one = g.layouts("v9.9.9-beta.1", rev, dist, imgs, t / "l1")
            two = g.layouts("v9.9.9-beta.1", rev, dist, imgs, t / "l2")
            bare = g.layouts("v9.9.9-beta.1", rev, dist, None, t / "l3")
        if one != two or [r.split()[0] for r in one] != ["podrunner", "runner", "runner-full"] \
                or one[0] != f"podrunner {d1}" or bare != one[:1]:
            bad.append(f"the layouts: {one} {two} {bare}")
        if g.read_layout(t / "l1" / "runner-full")[0] != "v9.9.9-beta.1-full":
            bad.append("the tag of runner-full")
        (imgs / rb.name).unlink()
        with contextlib.redirect_stderr(io.StringIO()):
            refused("the layouts with no aarch64 runner image", FileNotFoundError, "runner-aarch64.tar", g.layouts,
                    "v1", rev, dist, imgs, t / "l4")
        # The command lines of probe-push and layouts that stop before a
        # registry or a file is read.
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            codes = [g.main(["probe-push", "--registry", base, "--repo"]),
                     g.main(["probe-push", "--registry", base, "--name", "podutils/x"]),
                     g.main(["layouts", "--tag", "v1", "--revision", rev, "--out", str(t / "l5")])]
        if codes != [2, 2, 2]:
            bad.append(f"the exit codes of probe-push and layouts: {codes}")
        refused("a bad repository", ValueError, "'Podutils/x' is not a repository of a registry", g.main,
                ["probe-push", "--registry", base, "--repo", "Podutils/x"])
    for b in bad:
        print(f"self-test: {b}")
    if not bad:
        print("ghcr.py: self-test passes")
    return 1 if bad else 0
