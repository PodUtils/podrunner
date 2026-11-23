#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""Check C1 of the hosted snapshot (T-542): the layers hold the root 1:1.

    python3 -I scripts/hosted/c1.py compare EXPECTED ROWS...
    python3 -I scripts/hosted/c1.py --self-test

EXPECTED is the manifest of the members after the scrub (`plan.py
--manifest`): one row for each path, from lstat. Each ROWS file is the
manifest of one layer stream, in the order of the layers (`scrub.py verify
--rows`). The rows of the layers merge, an upper layer over a lower one,
and each path of EXPECTED must be there with the same type, mode, owner,
size, link target and file capabilities, and no other path. A row is:
path, type, mode, uid, gid, size, link, xattrs (TAB-separated). The content
of a file is not compared here: tar reads the same bytes that it lists.
"""

import os
import stat
import sys
import tarfile

# The extended attributes that a job sees: file capabilities and the
# attributes of a user. The capture writes no ACL record until podvm takes
# the fix of pod-extract (COORD-228); an ACL is not compared here.
def kept(name: str) -> bool:
    return name == "security.capability" or name.startswith("user.")


def xattrs_of(path: str) -> str:
    try:
        names = sorted(n for n in os.listxattr(path, follow_symlinks=False) if kept(n))
    except OSError:
        return ""
    return ",".join(f"{n}={os.getxattr(path, n, follow_symlinks=False).hex()}" for n in names)


def row_of_stat(rel: str, full: str, st: os.stat_result) -> str | None:
    m = st.st_mode
    size = link = ""
    if stat.S_ISREG(m):
        kind, size = "f", str(st.st_size)
    elif stat.S_ISDIR(m):
        kind = "d"
    elif stat.S_ISLNK(m):
        kind, link = "l", os.readlink(full)
    elif stat.S_ISCHR(m) or stat.S_ISBLK(m):
        kind, size = ("c" if stat.S_ISCHR(m) else "b"), f"{os.major(st.st_rdev)},{os.minor(st.st_rdev)}"
    elif stat.S_ISFIFO(m):
        kind = "p"
    else:
        # A socket: tar does not carry one.
        return None
    xa = "" if kind == "l" else xattrs_of(full)
    return "\t".join([rel, kind, oct(m & 0o7777), str(st.st_uid), str(st.st_gid), size, link, xa])


def row_of_member(ti: tarfile.TarInfo) -> str:
    rel = ti.name.removeprefix("./").rstrip("/")
    size = link = ""
    if ti.isreg():
        kind, size = "f", str(ti.size)
    elif ti.islnk():
        # A second name of a hard link: a file, its size in its first name.
        kind, link = "h", ti.linkname.removeprefix("./")
    elif ti.isdir():
        kind = "d"
    elif ti.issym():
        kind, link = "l", ti.linkname
    elif ti.ischr() or ti.isblk():
        kind, size = ("c" if ti.ischr() else "b"), f"{ti.devmajor},{ti.devminor}"
    elif ti.isfifo():
        kind = "p"
    else:
        kind = "?"
    xa = []
    for k, v in ti.pax_headers.items():
        if k.startswith("SCHILY.xattr.") and kept(k.removeprefix("SCHILY.xattr.")):
            xa.append(f"{k.removeprefix('SCHILY.xattr.')}={v.encode('utf-8', 'surrogateescape').hex()}")
    return "\t".join([rel, kind, oct(ti.mode & 0o7777), str(ti.uid), str(ti.gid), size, link, ",".join(sorted(xa))])


def load(path: str) -> dict[str, list[str]]:
    out = {}
    with open(path, encoding="utf-8", errors="surrogateescape") as f:
        for line in f:
            r = line.rstrip("\n").split("\t")
            out[r[0]] = r
    return out


FIELDS = ("path", "type", "mode", "uid", "gid", "size", "link", "xattrs")


def compare(expected: dict[str, list[str]], layers: list[dict[str, list[str]]]) -> list[str]:
    merged: dict[str, list[str]] = {}
    for rows in layers:
        merged.update(rows)
    bad = []
    for p, want in expected.items():
        got = merged.get(p)
        if got is None:
            bad.append(f"{p}: not in any layer")
            continue
        for i in range(1, len(FIELDS)):
            w, g = want[i], got[i]
            if got[1] == "h" and FIELDS[i] in ("type", "size", "link", "xattrs"):
                # The second name of a hard link: a file; its size, its
                # content and its xattrs are those of its first name.
                if FIELDS[i] == "type" and w != "f":
                    bad.append(f"{p}: type {w}, a hard link in the layer")
                continue
            if w != g:
                bad.append(f"{p}: {FIELDS[i]} {w!r} in the root, {g!r} in the layers")
    for p in merged.keys() - expected.keys():
        bad.append(f"{p}: in a layer, not in the root after the scrub")
    return bad


def self_test() -> int:
    import subprocess
    import tempfile

    bad = []
    with tempfile.TemporaryDirectory() as t:
        root = os.path.join(t, "root")
        os.makedirs(os.path.join(root, "usr/bin"))
        os.makedirs(os.path.join(root, "etc"))
        files = {"usr/bin/tool": b"tool\n", "usr/bin/setuid": b"x\n", "etc/conf": b"c\n"}
        for p, c in files.items():
            with open(os.path.join(root, p), "wb") as f:
                f.write(c)
        os.chmod(os.path.join(root, "usr/bin/setuid"), 0o4755)
        # A sparse file, stored in full (no --sparse, T-599): its row has
        # the apparent size, not the blocks.
        with open(os.path.join(root, "usr/bin/holes"), "wb") as f:
            f.truncate(1 << 20)
            f.seek(0, os.SEEK_END)
            f.write(b"end")
        os.link(os.path.join(root, "usr/bin/tool"), os.path.join(root, "usr/bin/tool2"))
        os.symlink("tool", os.path.join(root, "usr/bin/alias"))
        os.mkfifo(os.path.join(root, "etc/fifo"))
        try:
            os.setxattr(os.path.join(root, "usr/bin/tool"), "user.podrunner.cap", b"\x01\x00")
        except OSError:
            pass
        names = []
        for dirpath, dirs, fs in os.walk(root):
            dirs.sort()
            for n in sorted(dirs) + sorted(fs):
                names.append(os.path.relpath(os.path.join(dirpath, n), root))
        expected = {}
        for p in names:
            full = os.path.join(root, p)
            r = row_of_stat(p, full, os.lstat(full))
            if r:
                expected[p] = r.split("\t")
        # Two layers, as plan.py makes them: etc in one, usr in the other.
        def layer(sel):
            lst = os.path.join(t, "list")
            with open(lst, "wb") as f:
                f.write(b"".join(p.encode() + b"\0" for p in names if sel(p)))
            out = subprocess.run(
                ["tar", "-C", root, "--format=posix", "--xattrs", "--xattrs-include=*", "--numeric-owner", "--null",
                 "--no-recursion", "-T", lst, "-cf", "-"], check=True, capture_output=True).stdout
            import io
            with tarfile.open(fileobj=io.BytesIO(out), mode="r|*") as tf:
                return {r.split("\t")[0]: r.split("\t") for r in (row_of_member(ti) for ti in tf)}
        lower = layer(lambda p: p.startswith("etc"))
        upper = layer(lambda p: p.startswith("usr"))
        if upper["usr/bin/holes"][5] != str((1 << 20) + 3):
            bad.append(f"the size of a sparse file: {upper['usr/bin/holes'][5]}")
        found = compare(expected, [lower, upper])
        if found:
            bad.append(f"a whole capture: {found}")
        # A lost file, a lost setuid bit, a lost xattr and an extra path fail.
        lost = {k: v for k, v in upper.items() if k != "usr/bin/alias"}
        if not any("usr/bin/alias: not in any layer" in f for f in compare(expected, [lower, lost])):
            bad.append("a lost file passes")
        changed = dict(upper)
        changed["usr/bin/setuid"] = changed["usr/bin/setuid"][:2] + ["0o755"] + changed["usr/bin/setuid"][3:]
        if not any("usr/bin/setuid: mode" in f for f in compare(expected, [lower, changed])):
            bad.append("a lost setuid bit passes")
        if expected["usr/bin/tool"][7]:
            nox = dict(upper)
            nox["usr/bin/tool"] = nox["usr/bin/tool"][:7] + [""]
            if not any("usr/bin/tool: xattrs" in f for f in compare(expected, [lower, nox])):
                bad.append("a lost xattr passes")
        extra = dict(upper)
        extra["etc/shadow"] = ["etc/shadow", "f", "0o640", "0", "0", "1", "", ""]
        if not any("etc/shadow: in a layer" in f for f in compare(expected, [lower, extra])):
            bad.append("an extra path passes")
    for b in bad:
        print(f"self-test: {b}")
    if not bad:
        print("c1.py: self-test passes")
    return 1 if bad else 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    if len(sys.argv) < 4 or sys.argv[1] != "compare":
        print("usage: c1.py compare EXPECTED ROWS... | --self-test", file=sys.stderr)
        return 2
    found = compare(load(sys.argv[2]), [load(p) for p in sys.argv[3:]])
    for f in found[:200]:
        print(f)
    if len(found) > 200:
        print(f"... {len(found) - 200} more")
    print(f"c1: {len(found)} differences between the root after the scrub and the layers")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
