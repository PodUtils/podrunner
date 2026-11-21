#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The partition of the hosted snapshot into layers (T-542).

    python3 -I scripts/hosted/plan.py --root R --members M --out DIR [--layers F] [--digests D]
    python3 -I scripts/hosted/plan.py --self-test

`M` is the member list of `scrub.py list` (NUL-separated: the paths of the
root, then `--directory=FIX` and the fixed paths). Each path goes to the
layer of the first rule of `images/hosted/layers.toml` that matches it or a
parent directory; the rest is `base`, on top. Each name of a hard-link
group goes to the layer of the first name, so tar keeps the link. DIR gets
one list for each layer (`NN-NAME.list`, for GNU tar `--null
--no-verbatim-files-from --no-recursion -T`) and `plan.json`: the order,
the bytes, and a content key of each layer (the SHA-256 of its rows with
no time), so an unchanged layer can reuse its blob. `D` gives the SHA-256
of each file ("HEX  PATH" lines of sha256sum) for the key.
"""

import argparse
import hashlib
import json
import os
import pathlib
import re
import stat
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
LAYERS = ROOT / "images" / "hosted" / "layers.toml"
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from scrub import glob_re  # noqa: E402


def read_members(data: bytes) -> list[tuple[str, str]]:
    """(base, path) of each member: base is "" for the root, else the fix tree."""
    out, base = [], ""
    for n in data.split(b"\0"):
        if not n:
            continue
        s = n.decode()
        if s.startswith("--directory="):
            base = s.removeprefix("--directory=")
            continue
        out.append((base, s))
    return out


class Plan:
    def __init__(self, text: str):
        doc = tomllib.loads(text)
        self.max_bytes = doc["max_bytes"]
        self.max_layers = doc["max_layers"]
        self.rules = [(r["name"], glob_re(r["path"]), r.get("each", False)) for r in doc["layer"]]
        names = [r[0] for r in self.rules]
        if len(set(names)) != len(names) or "base" in names:
            raise ValueError("each layer rule needs a name of its own, and not base")

    def layer_of(self, path: str, is_dir) -> tuple[int, str]:
        parts = path.split("/")
        for i, (name, g, each) in enumerate(self.rules):
            for n in range(1, len(parts) + 1):
                anc = "/".join(parts[:n])
                if not g.match(anc):
                    continue
                if each:
                    # One layer for each matched directory; a file that
                    # matches is not a layer of its own.
                    if n == len(parts) and not is_dir(path):
                        continue
                    return i, f"{name}-{anc.split('/')[-1]}"
                return i, name
        return len(self.rules), "base"


def lstat_of(root: pathlib.Path, base: str, path: str) -> os.stat_result:
    return os.lstat((pathlib.Path(base) if base else root) / path)


def plan(root: pathlib.Path, members: list[tuple[str, str]], rules: Plan, digests: dict[str, str]) -> list[dict]:
    st = {(b, p): lstat_of(root, b, p) for b, p in members}
    # A hard-link group goes to the layer of its first name.
    first: dict[tuple[int, int], str] = {}
    layer: dict[tuple[str, str], tuple[int, str]] = {}
    for b, p in members:
        s = st[(b, p)]
        is_dir = lambda q, b=b: stat.S_ISDIR(lstat_of(root, b, q).st_mode)  # noqa: E731
        lay = rules.layer_of(p, is_dir)
        if stat.S_ISREG(s.st_mode) and s.st_nlink > 1 and not b:
            key = (s.st_dev, s.st_ino)
            if key in first:
                lay = layer[("", first[key])]
            else:
                first[key] = p
        layer[(b, p)] = lay
    groups: dict[tuple[int, str], list[tuple[str, str]]] = {}
    for m in members:
        groups.setdefault(layer[m], []).append(m)
    out = []
    for (order, name), ms in sorted(groups.items()):
        h = hashlib.sha256()
        size = 0
        for b, p in sorted(ms, key=lambda m: m[1]):
            s = st[(b, p)]
            full = (pathlib.Path(b) if b else root) / p
            link = os.readlink(full) if stat.S_ISLNK(s.st_mode) else ""
            if stat.S_ISREG(s.st_mode):
                size += s.st_blocks * 512 if s.st_blocks * 512 < s.st_size else s.st_size
            row = [p, oct(s.st_mode), str(s.st_uid), str(s.st_gid), str(s.st_size), link, digests.get(p, "")]
            h.update("\t".join(row).encode() + b"\n")
        if size > rules.max_bytes:
            # Name the largest trees of the layer: the next rule of layers.toml.
            by: dict[str, int] = {}
            for b, p in ms:
                s = st[(b, p)]
                if stat.S_ISREG(s.st_mode):
                    key = "/".join(p.split("/")[:3])
                    by[key] = by.get(key, 0) + min(s.st_size, s.st_blocks * 512 or s.st_size)
            top = ", ".join(f"{k} {v >> 20} MiB" for k, v in sorted(by.items(), key=lambda kv: -kv[1])[:8])
            raise ValueError(f"layer {name}: {size} bytes, over {rules.max_bytes}: split it in layers.toml ({top})")
        out.append({"order": order, "name": name, "members": ms, "bytes": size, "key": h.hexdigest()})
    if len(out) > rules.max_layers:
        raise ValueError(f"{len(out)} layers, over {rules.max_layers}: join some in layers.toml")
    return out


def write_manifest(root: pathlib.Path, members: list[tuple[str, str]], out: pathlib.Path) -> None:
    """The rows of check C1 for each member: what the layers must hold."""
    from c1 import row_of_stat

    with open(out, "w", encoding="utf-8", errors="surrogateescape") as f:
        for b, p in members:
            full = str((pathlib.Path(b) if b else root) / p)
            r = row_of_stat(p, full, os.lstat(full))
            if r is not None:
                f.write(r + "\n")


def write(out_dir: pathlib.Path, layers: list[dict]) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    meta = []
    for i, lay in enumerate(layers):
        fname = f"{i:02d}-{re.sub(r'[^A-Za-z0-9._-]', '_', lay['name'])}.list"
        with open(out_dir / fname, "wb") as f:
            base = ""
            for b, p in sorted(lay["members"], key=lambda m: (m[0] != "", m[1])):
                if b != base:
                    f.write(f"--directory={b}\0".encode())
                    base = b
                f.write(p.encode() + b"\0")
        meta.append({"name": lay["name"], "list": fname, "count": len(lay["members"]), "bytes": lay["bytes"],
                     "key": lay["key"]})
    (out_dir / "plan.json").write_text(json.dumps({"layers": meta}, indent=1) + "\n")


def self_test() -> int:
    import subprocess
    import tempfile

    bad = []
    with tempfile.TemporaryDirectory() as t:
        t = pathlib.Path(t)
        root, fix, out, got = t / "root", t / "fix", t / "out", t / "got"
        files = {
            "usr/bin/tool": b"a tool\n",
            "usr/bin/setuid": b"#!/bin/sh\n",
            "usr/lib/jvm/java-21/bin/java": b"java\n",
            "opt/hostedtoolcache/Python/3.12.1/x64/bin/python": b"python\n",
            "opt/hostedtoolcache/Python/3.12.1.complete": b"",
            "opt/hostedtoolcache/node/22.11.0/x64/bin/node": b"node\n",
            "etc/hosts": b"127.0.0.1 localhost\n",
            "etc/machine-id": b"live-id\n",
        }
        for p, c in files.items():
            (root / p).parent.mkdir(parents=True, exist_ok=True)
            (root / p).write_bytes(c)
        os.chmod(root / "usr/bin/setuid", 0o4755)
        # A hard link across two planned layers: jvm and base.
        os.link(root / "usr/lib/jvm/java-21/bin/java", root / "usr/bin/java-link")
        with open(root / "usr/bin/sparse", "wb") as f:
            f.seek(1 << 20)
            f.write(b"end")
        os.symlink("tool", root / "usr/bin/alias")
        try:
            os.setxattr(root / "usr/bin/tool", "user.podrunner.test", b"kept")
            xattr = True
        except OSError:
            xattr = False
        (fix / "etc").mkdir(parents=True)
        (fix / "etc/machine-id").write_bytes(b"")
        names = []
        for dirpath, dirs, fs in os.walk(root):
            dirs.sort()
            for n in sorted(dirs) + sorted(fs):
                p = (pathlib.Path(dirpath) / n).relative_to(root).as_posix()
                if p != "etc/machine-id":
                    names.append(p)
        members = b"".join(p.encode() + b"\0" for p in names) + f"--directory={fix}\0etc/machine-id\0".encode()
        layers = plan(root, read_members(members), Plan(LAYERS.read_text()), {})
        write(out, layers)
        by = {lay["name"]: {p for _, p in lay["members"]} for lay in layers}
        together = [n for n, ps in by.items() if {"usr/bin/java-link", "usr/lib/jvm/java-21/bin/java"} <= ps]
        if not together:
            bad.append(f"the names of a hard link are in two layers: {sorted(by)}")
        if not {"toolcache-3.12.1", "toolcache-22.11.0", "base"} <= set(by):
            bad.append(f"the layers: {sorted(by)}")
        if any("3.12.1.complete" in p for n, ps in by.items() if n.startswith("toolcache") for p in ps):
            bad.append("a file of an each rule is a layer of its own")
        if [lay["name"] for lay in layers][-1] != "base":
            bad.append("base is not on top")
        # The round trip: each list through GNU tar, extracted in order.
        got.mkdir()
        meta = json.loads((out / "plan.json").read_text())["layers"]
        for m in meta:
            tar = t / (m["list"] + ".tar")
            # The flags of the capture (capture.sh): no --acls until podvm
            # takes the fix of pod-extract (COORD-228).
            subprocess.run(
                ["tar", "-C", str(root), "--format=posix", "--xattrs", "--xattrs-include=*", "--sparse",
                 "--numeric-owner", "--null", "--no-verbatim-files-from", "--no-recursion", "-T",
                 str(out / m["list"]), "-cf", str(tar)], check=True)
            # -p: a consumer keeps the modes (setuid) as root does by default.
            subprocess.run(["tar", "-C", str(got), "-p", "--xattrs", "--xattrs-include=*", "-xf", str(tar)], check=True)
        expect = dict(files) | {"etc/machine-id": b""}
        for p, c in expect.items():
            if (got / p).read_bytes() != c:
                bad.append(f"round trip: {p}")
        if os.stat(got / "usr/bin/setuid").st_mode & 0o7777 != 0o4755:
            bad.append("round trip: the setuid bit")
        if (got / "usr/bin/sparse").read_bytes() != b"\0" * (1 << 20) + b"end":
            bad.append("round trip: the sparse file")
        if os.readlink(got / "usr/bin/alias") != "tool":
            bad.append("round trip: the link")
        if os.stat(got / "usr/bin/java-link").st_ino != os.stat(got / "usr/lib/jvm/java-21/bin/java").st_ino:
            bad.append("round trip: the hard link")
        if xattr and os.getxattr(got / "usr/bin/tool", "user.podrunner.test") != b"kept":
            bad.append("round trip: the xattr")
        # The key does not change with a time, and changes with content.
        k1 = {m["name"]: m["key"] for m in meta}
        os.utime(root / "usr/bin/tool", (1, 1))
        k2 = {lay["name"]: lay["key"] for lay in plan(root, read_members(members), Plan(LAYERS.read_text()), {})}
        if k1 != k2:
            bad.append("a time changes the key")
        k3 = {lay["name"]: lay["key"] for lay in
              plan(root, read_members(members), Plan(LAYERS.read_text()), {"usr/bin/tool": "ab"})}
        if k3["base"] == k1["base"] or k3["toolcache-22.11.0"] != k1["toolcache-22.11.0"]:
            bad.append("a digest changes the wrong key")
    for b in bad:
        print(f"self-test: {b}")
    if not bad:
        print("plan.py: self-test passes" + ("" if xattr else " (no user xattrs on this file system)"))
    return 1 if bad else 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=pathlib.Path, required=True)
    ap.add_argument("--members", type=pathlib.Path, required=True)
    ap.add_argument("--out", type=pathlib.Path, required=True)
    ap.add_argument("--layers", type=pathlib.Path, default=LAYERS)
    ap.add_argument("--digests", type=pathlib.Path)
    ap.add_argument("--manifest", type=pathlib.Path, help="the rows of check C1 (c1.py)")
    a = ap.parse_args()
    digests = {}
    if a.digests:
        for line in a.digests.read_text().splitlines():
            h, _, p = line.partition("  ")
            digests[p.removeprefix("./")] = h
    members = read_members(a.members.read_bytes())
    layers = plan(a.root, members, Plan(a.layers.read_text()), digests)
    if a.manifest:
        write_manifest(a.root, members, a.manifest)
    write(a.out, layers)
    for lay in layers:
        print(f"{lay['name']}\t{len(lay['members'])}\t{lay['bytes']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
