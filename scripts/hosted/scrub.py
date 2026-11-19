#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The scrub of the hosted snapshot (T-542): images/hosted/scrub.toml.

    python3 -I scripts/hosted/scrub.py fix --root R --out FIX [--rules F]
    python3 -I scripts/hosted/scrub.py list --root R --fix FIX [--runner-root P] [--rules F] > members
    python3 -I scripts/hosted/scrub.py verify [--runner-root P] [--rules F] < layer.tar
    python3 -I scripts/hosted/scrub.py --self-test

`fix` writes the fix tree: each [[fix]] file with its new content and the
owner and the mode of the live file, and each [[mask]] link. `list` writes
the members of a layer for GNU tar (`--null --no-verbatim-files-from
--no-recursion -T`): each path of the root that no rule drops, and each
fixed path from the fix tree (`--directory=FIX`), so a dropped or an old
file never enters a blob. `verify` reads a tar stream (pass 3 and C2): a
dropped path, a fixed path with other content, or a file of a scan root
with a credential pattern that no rule covers fails it. It names the path
and the pattern, never the match.
"""

import argparse
import os
import pathlib
import re
import sys
import tarfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
RULES = ROOT / "images" / "hosted" / "scrub.toml"

# The patterns of a credential (C2). A match prints the name only.
PATTERNS = {
    "private-key": re.compile(rb"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    "github-token": re.compile(rb"\b(gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{40,})"),
    "json-secret": re.compile(
        rb'"(authToken|accessToken|access_token|refresh_token|token|password|secret|sasUri|diagnosticsSasUri)"'
        rb'\s*:\s*"[^"]{16,}"'
    ),
    "sas-signature": re.compile(rb"[?&]sig=[A-Za-z0-9%+/=]{20,}"),
    "aws-key": re.compile(rb"\bAKIA[0-9A-Z]{16}\b"),
    "docker-auth": re.compile(rb'"auth"\s*:\s*"[A-Za-z0-9+/=]{16,}"'),
}
SCAN_MAX = 1 << 20


def glob_re(g: str) -> re.Pattern:
    out, i = "", 0
    while i < len(g):
        if g.startswith("**/", i):
            out, i = out + "(?:.*/)?", i + 3
        elif g.startswith("**", i):
            out, i = out + ".*", i + 2
        elif g[i] == "*":
            out, i = out + "[^/]*", i + 1
        elif g[i] == "?":
            out, i = out + "[^/]", i + 1
        elif g[i] == "[":
            j = g.index("]", i)
            out, i = out + g[i : j + 1], j + 1
        else:
            out, i = out + re.escape(g[i]), i + 1
    return re.compile(out + r"\Z")


class Rules:
    def __init__(self, text: str, runner_root: str | None = None):
        doc = tomllib.loads(text)
        for kind in ("drop", "fix", "mask", "allow", "skip"):
            for i, r in enumerate(doc.get(kind, [])):
                if not r.get("reason"):
                    raise ValueError(f"{kind} rule {i + 1}: no reason")
        self.drop = [glob_re(r["path"]) for r in doc.get("drop", [])]
        if runner_root:
            self.drop.append(glob_re(runner_root.strip("/")))
        self.fix = [(glob_re(r["path"]), r) for r in doc.get("fix", [])]
        self.mask = [r["unit"] for r in doc.get("mask", [])]
        self.scan = [glob_re(r["path"]) for r in doc.get("scan", [])]
        self.allow = [glob_re(r["path"]) for r in doc.get("allow", [])]
        self.skip = [glob_re(r["path"]) for r in doc.get("skip", [])]

    def dropped(self, path: str) -> bool:
        # A rule on a directory covers its tree.
        parts = path.split("/")
        return any(r.match("/".join(parts[:n])) for r in self.drop for n in range(1, len(parts) + 1))

    def fix_of(self, path: str) -> dict | None:
        if path in self.masked():
            return {"mask": True}
        return next((r for g, r in self.fix if g.match(path)), None)

    def masked(self) -> set[str]:
        return {f"etc/systemd/system/{u}" for u in self.mask}

    def scanned(self, path: str) -> bool:
        if not any(r.match(path) for r in self.scan) or any(r.match(path) for r in self.allow):
            return False
        # A skip rule on a directory covers its tree: code and documents of
        # packages and public images, not the state of a machine.
        parts = path.split("/")
        return not any(r.match("/".join(parts[:n])) for r in self.skip for n in range(1, len(parts) + 1))


def walk(root: pathlib.Path):
    """Each path of `root` (relative, sorted), on one file system."""
    dev = root.stat().st_dev
    for dirpath, dirs, files in os.walk(root):
        dirs.sort()
        d = pathlib.Path(dirpath)
        keep = []
        for n in dirs:
            p = d / n
            if p.is_symlink() or p.lstat().st_dev == dev:
                keep.append(n)
            yield p.relative_to(root).as_posix()
        dirs[:] = [n for n in keep if not (d / n).is_symlink()]
        for n in sorted(files):
            yield (d / n).relative_to(root).as_posix()


def cmd_fix(rules: Rules, root: pathlib.Path, out: pathlib.Path) -> list[str]:
    done = []
    for path in walk(root):
        r = rules.fix_of(path)
        if r and not r.get("mask") and not rules.dropped(path) and (root / path).is_file():
            done.append(write_fix(root, out, path, r["content"]))
    for g, r in rules.fix:
        path = r["path"]
        if r.get("create") and not (root / path).exists():
            done.append(write_fix(root, out, path, r["content"]))
    for path in sorted(rules.masked()):
        dst = out / path
        dst.parent.mkdir(parents=True, exist_ok=True)
        if dst.is_symlink() or dst.exists():
            dst.unlink()
        dst.symlink_to("/dev/null")
        done.append(path)
    return sorted(set(done))


def write_fix(root: pathlib.Path, out: pathlib.Path, path: str, content: str) -> str:
    dst = out / path
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_bytes(content.encode())
    live = root / path
    if live.exists() and not live.is_symlink():
        st = live.stat()
        os.chmod(dst, st.st_mode & 0o7777)
        if os.geteuid() == 0:
            os.chown(dst, st.st_uid, st.st_gid)
    else:
        os.chmod(dst, 0o644)
    return path


def cmd_list(rules: Rules, root: pathlib.Path, fix: pathlib.Path, w) -> None:
    fixed = set()
    for dirpath, _, files in os.walk(fix):
        for n in files + [d for d in os.listdir(dirpath) if (pathlib.Path(dirpath) / d).is_symlink()]:
            fixed.add((pathlib.Path(dirpath) / n).relative_to(fix).as_posix())
    names = []
    for path in walk(root):
        if path.startswith("-"):
            raise ValueError(f"a path that tar reads as an option: {path!r}")
        if rules.dropped(path) or path in fixed:
            continue
        names.append(path)
    w.write(b"".join(p.encode() + b"\0" for p in names))
    # A directory of the fix tree that the root does not have is a member
    # too, with its own mode.
    dirs = {"/".join(p.split("/")[:n]) for p in fixed for n in range(1, p.count("/") + 1)}
    extra = {d for d in dirs if not (root / d).exists()}
    w.write(f"--directory={fix}\0".encode())
    w.write(b"".join(p.encode() + b"\0" for p in sorted(fixed | extra)))


def cmd_verify(rules: Rules, stream) -> list[str]:
    bad = []
    # A large buffer: the stream of a layer can be 8 GB.
    with tarfile.open(fileobj=stream, mode="r|*", bufsize=1 << 20) as tar:
        for m in tar:
            path = m.name.removeprefix("./").rstrip("/")
            if rules.dropped(path):
                bad.append(f"{path}: a dropped path is in the stream")
                continue
            r = rules.fix_of(path)
            data = None
            if m.isfile() and (r or (rules.scanned(path) and m.size <= SCAN_MAX)):
                data = tar.extractfile(m).read()
            if r and r.get("mask"):
                if not (m.issym() and m.linkname == "/dev/null"):
                    bad.append(f"{path}: not masked")
                continue
            if r is not None:
                if data != r["content"].encode():
                    bad.append(f"{path}: not the content of its fix rule")
                continue
            if data is not None:
                for name, pat in PATTERNS.items():
                    if pat.search(data):
                        bad.append(f"{path}: a {name} pattern that no rule covers")
    return bad


def self_test() -> int:
    import io
    import tempfile

    bad = []
    rules_text = RULES.read_text()
    with tempfile.TemporaryDirectory() as t:
        t = pathlib.Path(t)
        root, fix = t / "root", t / "fix"
        files = {
            "etc/machine-id": "0123456789abcdef0123456789abcdef\n",
            "etc/hostname": "fv-az123-456\n",
            "etc/fstab": "UUID=x / ext4 defaults 0 1\n",
            "etc/ssh/ssh_host_ed25519_key": "-----BEGIN OPENSSH PRIVATE KEY-----\nmade-up\n",
            "etc/ssh/sshd_config": "Port 22\n",
            "etc/systemd/system/hosted-compute-agent.service": "[Service]\nExecStart=/opt/hca/x\n",
            "opt/hca/.settings": '{"authToken": "made-up-made-up-made-up"}\n',
            "home/runner/actions-runner/cached/2.0.0/.credentials": '{"token": "made-up-made-up-made-up"}\n',
            "home/runner/.docker/config.json": '{"auths": {"x": {"auth": "bWFkZS11cDptYWRlLXVw"}}}\n',
            "home/runner/.bashrc": "export PATH\n",
            "var/log/syslog": "a log line\n",
            "var/log/cloud-init.log": "a log line\n",
            "var/log/syslog.1": "an old line\n",
            "usr/bin/tool": "a binary\n",
            "var/lib/mysql/server-key.pem": "-----BEGIN PRIVATE KEY-----\nmade-up\n",
            "var/lib/mysql/ibdata1": "data\n",
        }
        for p, c in files.items():
            (root / p).parent.mkdir(parents=True, exist_ok=True)
            (root / p).write_text(c)
        rules = Rules(rules_text, "home/runner/actions-runner/cached/2.0.0")
        cmd_fix(rules, root, fix)
        members = io.BytesIO()
        cmd_list(rules, root, fix, members)
        names = members.getvalue().split(b"\0")
        listed = {n.decode() for n in names if n and not n.startswith(b"--")}
        for gone in ("etc/ssh/ssh_host_ed25519_key", "opt/hca/.settings", "var/log/syslog.1", "var/lib/mysql/server-key.pem",
                     "home/runner/actions-runner/cached/2.0.0/.credentials"):
            if gone in listed:
                bad.append(f"list: {gone} is listed")
        for kept in ("etc/machine-id", "home/runner/.docker/config.json", "usr/bin/tool", "etc/ssh/sshd_config", "var/lib/mysql/ibdata1",
                     "etc/cloud/cloud-init.disabled", "etc/systemd/system/hosted-compute-agent.service"):
            if kept not in listed:
                bad.append(f"list: {kept} is not listed")
        from_fix = {n.decode() for n in names[names.index(next(n for n in names if n.startswith(b"--"))) + 1 :] if n}
        if "etc/systemd/system/ssh.service.d" not in from_fix or "etc/systemd" in from_fix:
            bad.append("list: the directories of the fix tree")
        if (fix / "etc/machine-id").read_bytes() != b"":
            bad.append("fix: etc/machine-id is not empty")
        if os.readlink(fix / "etc/systemd/system/hosted-compute-agent.service") != "/dev/null":
            bad.append("fix: the agent is not masked")

        def tar_of(pairs) -> io.BytesIO:
            b = io.BytesIO()
            with tarfile.open(fileobj=b, mode="w", format=tarfile.PAX_FORMAT) as w:
                for base, p in pairs:
                    w.add(base / p, arcname=p, recursive=False)
            b.seek(0)
            return b

        # The scrubbed layer passes; the raw root fails on each secret.
        cur, pairs = root, []
        for n in names:
            if not n:
                continue
            s = n.decode()
            if s.startswith("--directory="):
                cur = pathlib.Path(s.removeprefix("--directory="))
                continue
            pairs.append((cur, s))
        found = cmd_verify(rules, tar_of(pairs))
        if found:
            bad.append(f"verify of the scrubbed layer: {found}")
        raw = cmd_verify(rules, tar_of([(root, p) for p in sorted(walk(root))]))
        for want in ("etc/ssh/ssh_host_ed25519_key: a dropped path", "opt/hca/.settings: a dropped path",
                     "etc/machine-id: not the content", "hosted-compute-agent.service: not masked"):
            if not any(want in f for f in raw):
                bad.append(f"verify of the raw root: no {want!r} in {raw}")
        # An unlisted secret in a scan root fails, with its name only.
        (root / "etc/new-agent.json").write_text('{"password": "made-up-made-up-made-up"}\n')
        found = cmd_verify(rules, tar_of(pairs + [(root, "etc/new-agent.json")]))
        if found != ["etc/new-agent.json: a json-secret pattern that no rule covers"]:
            bad.append(f"verify of an unlisted secret: {found}")
        # A skipped tree is not scanned: library code names key headers.
        (root / "var/lib/gems/x").mkdir(parents=True)
        (root / "var/lib/gems/x/README.md").write_text("-----BEGIN RSA PRIVATE KEY-----\n")
        if cmd_verify(rules, tar_of(pairs + [(root, "var/lib/gems/x/README.md")])):
            bad.append("a skipped tree is scanned")
        if any("made-up" in f for f in found + raw):
            bad.append("a finding prints a secret")
    for b in bad:
        print(f"self-test: {b}")
    if not bad:
        print("scrub.py: self-test passes")
    return 1 if bad else 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["fix", "list", "verify"])
    ap.add_argument("--rules", type=pathlib.Path, default=RULES)
    ap.add_argument("--root", type=pathlib.Path)
    ap.add_argument("--out", type=pathlib.Path)
    ap.add_argument("--fix", type=pathlib.Path)
    ap.add_argument("--runner-root")
    a = ap.parse_args()
    rules = Rules(a.rules.read_text(), a.runner_root)
    if a.cmd == "fix":
        for p in cmd_fix(rules, a.root, a.out):
            print(p)
        return 0
    if a.cmd == "list":
        cmd_list(rules, a.root, a.fix, sys.stdout.buffer)
        return 0
    found = cmd_verify(rules, sys.stdin.buffer)
    for f in found:
        print(f, file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
