#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The runner layer of the hosted snapshot (T-541).

    python3 -I scripts/hosted/runner-layer.py url --runner-toml FILE --arch ARCH
    python3 -I scripts/hosted/runner-layer.py make --runner-toml FILE --arch ARCH \\
        --tarball TGZ --environment ETC_ENVIRONMENT --passwd ETC_PASSWD \\
        --hook FILE --job-started FILE --license FILE --out LAYER.TAR
    python3 -I scripts/hosted/runner-layer.py --self-test

The published snapshot holds no runner: the capture drops the hosted
runner root and /home/runner/work (images/hosted/scrub.toml). This layer
goes on top of the snapshot (`oci.py append`) and gives a unit of it:

- the official runner of images/runner/runner.toml at
  /home/runner/actions-runner, one path for both archs, owned by `runner`
  (its uid and its group of the snapshot's /etc/passwd);
- `.env` and `.path` of the runner, made from the snapshot's
  /etc/environment: a JIT runner runs no `config.sh`, so nothing else
  writes them, and the runner sets each line in the environment of each
  job (the hosted PATH, ImageOS, ImageVersion, RUNNER_TOOL_CACHE,
  AGENT_TOOLSDIRECTORY);
- the hooks of images/runner/Dockerfile, and the links of the credential
  files into /podrunner/.runner, where the unit writes them;
- the empty work folder /home/runner/work, the hosted path; the kind sets
  it in `.runner` (`[unit.KIND] work_folder`).

ARCH is x86_64, aarch64, amd64 or arm64. `url` prints the URL of the
tarball of the release. `make` checks the tarball against the digest of
runner.toml, writes the layer as an uncompressed POSIX tar with no time
(the same inputs give the same bytes), and prints its diff id.
"""

import argparse
import hashlib
import io
import posixpath
import re
import sys
import tarfile
import tomllib

ROOT = "home/runner/actions-runner"
WORK = "home/runner/work"
HOME = "/home/runner"
# Where the unit writes the credential files (the incus SCRIPT, the podvm
# driver). Relative links: on the supervise rung of podvm an absolute link
# resolves on the host.
CREDENTIALS = "/podrunner/.runner"
LINKS = (".runner", ".credentials", ".credentials_rsaparams", ".podrunner-job")
JOB_STARTED = "hooks/podrunner-job-started.sh"
HOOK = "hooks/podrunner-hook.sh"
LICENSE = "LICENSE.actions-runner"
# The lines of /etc/environment that make a hosted job: a snapshot with
# no such line is not a hosted image.
REQUIRED = ("PATH", "ImageOS", "ImageVersion", "RUNNER_TOOL_CACHE", "AGENT_TOOLSDIRECTORY")
KEY = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
ARCH = {"x86_64": ("x86_64", "x64"), "amd64": ("x86_64", "x64"), "aarch64": ("aarch64", "arm64"),
        "arm64": ("aarch64", "arm64")}


def pin(toml_text: str, arch: str) -> tuple[str, str, str]:
    """The version, the sha256 and the URL of the tarball of ARCH."""
    if arch not in ARCH:
        raise ValueError(f"no runner of the arch {arch!r}")
    key, name = ARCH[arch]
    t = tomllib.loads(toml_text)
    version, digest = t["version"], t["sha256"][key]
    url = (f"https://github.com/actions/runner/releases/download/v{version}/"
           f"actions-runner-linux-{name}-{version}.tar.gz")
    return version, digest, url


def parse_environment(text: str) -> list[tuple[str, str]]:
    """The lines KEY=VALUE of /etc/environment, in order; a quoted value
    loses its quotes. An error names the line, never a value."""
    env: dict[str, str] = {}
    for n, line in enumerate(text.splitlines(), 1):
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        key, eq, value = s.partition("=")
        if not eq or not KEY.fullmatch(key):
            raise ValueError(f"/etc/environment line {n}: not KEY=VALUE")
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
            value = value[1:-1]
        env[key] = value
    missing = [k for k in REQUIRED if not env.get(k)]
    if missing:
        raise ValueError(f"/etc/environment has no {', '.join(missing)}: not a hosted image")
    return list(env.items())


def runner_ids(passwd: str) -> tuple[int, int]:
    """The uid and the group of `runner` in /etc/passwd."""
    for line in passwd.splitlines():
        f = line.split(":")
        if len(f) >= 7 and f[0] == "runner":
            if f[5] != HOME:
                raise ValueError(f"the home of runner is not {HOME}")
            return int(f[2]), int(f[3])
    raise ValueError("/etc/passwd has no user runner")


def runner_members(tgz: bytes) -> list[tuple[tarfile.TarInfo, bytes | None]]:
    """The members of the tarball, each name relative to the runner root.
    A member out of the root, a link out of it or a special file is
    refused."""
    out = []
    with tarfile.open(fileobj=io.BytesIO(tgz), mode="r:gz") as t:
        for m in t.getmembers():
            name = posixpath.normpath(m.name)
            if name == ".":
                continue
            if m.name.startswith("/") or name == ".." or name.startswith("../"):
                raise ValueError(f"the tarball member {m.name!r} is out of the runner root")
            if not (m.isreg() or m.isdir() or m.issym() or m.islnk()):
                raise ValueError(f"the tarball member {m.name!r} is not a file, a directory or a link")
            if m.issym():
                to = posixpath.normpath(posixpath.join(posixpath.dirname(name), m.linkname))
                if m.linkname.startswith("/") or to == ".." or to.startswith("../"):
                    raise ValueError(f"the link {m.name!r} points out of the runner root")
            if m.islnk():
                m.linkname = posixpath.normpath(m.linkname)
            m.name = name
            out.append((m, t.extractfile(m).read() if m.isreg() else None))
    return out


def entry(name: str, kind: bytes, mode: int, ids: tuple[int, int], data: bytes | None = None,
          link: str = "") -> tuple[tarfile.TarInfo, bytes | None]:
    ti = tarfile.TarInfo(name)
    ti.type, ti.mode, ti.mtime, ti.linkname = kind, mode, 0, link
    ti.uid, ti.gid = ids
    ti.uname = ti.gname = ""
    ti.size = len(data) if data is not None else 0
    return ti, data


def make(toml_text: str, arch: str, tgz: bytes, environment: str, passwd: str, hook: bytes,
         job_started: bytes, licence: bytes) -> bytes:
    version, digest, _ = pin(toml_text, arch)
    got = hashlib.sha256(tgz).hexdigest()
    if got != digest:
        raise ValueError(f"the tarball of the runner {version} has the sha256 {got}, not {digest} of runner.toml")
    env = parse_environment(environment)
    if any(k == "ACTIONS_RUNNER_HOOK_JOB_STARTED" for k, _ in env):
        raise ValueError("/etc/environment names ACTIONS_RUNNER_HOOK_JOB_STARTED: the layer sets it")
    ids = runner_ids(passwd)
    path = dict(env)["PATH"]
    env.append(("ACTIONS_RUNNER_HOOK_JOB_STARTED", f"/{ROOT}/{JOB_STARTED}"))
    dot_env = "".join(f"{k}={v}\n" for k, v in env).encode()
    ours = {
        ".env": entry(".env", tarfile.REGTYPE, 0o644, ids, dot_env),
        ".path": entry(".path", tarfile.REGTYPE, 0o644, ids, (path + "\n").encode()),
        "hooks": entry("hooks", tarfile.DIRTYPE, 0o755, ids),
        HOOK: entry(HOOK, tarfile.REGTYPE, 0o755, ids, hook),
        JOB_STARTED: entry(JOB_STARTED, tarfile.REGTYPE, 0o755, ids, job_started),
        LICENSE: entry(LICENSE, tarfile.REGTYPE, 0o644, ids, licence),
    }
    for f in LINKS:
        to = posixpath.relpath(f"{CREDENTIALS}/{f}", f"/{ROOT}")
        ours[f] = entry(f, tarfile.SYMTYPE, 0o777, ids, link=to)
    members = {}
    for m, data in runner_members(tgz):
        if m.name in ours:
            raise ValueError(f"the tarball has {m.name!r}, a path of this layer")
        m.uid, m.gid = ids
        m.uname = m.gname = ""
        m.mtime = 0
        m.mode &= 0o7777
        m.pax_headers = {}
        if m.islnk():
            m.linkname = f"{ROOT}/{m.linkname}"
        members[m.name] = (m, data)
    members.update(ours)
    out = {}
    for rel, (m, data) in members.items():
        m.name = f"{ROOT}/{rel}"
        out[m.name] = (m, data)
        # Each parent in the root is a directory of runner, as the chown -R
        # of images/runner/Dockerfile leaves it.
        parent = posixpath.dirname(m.name)
        while parent != ROOT and parent not in out and parent[len(ROOT) + 1:] not in members:
            out[parent] = entry(parent, tarfile.DIRTYPE, 0o755, ids)
            parent = posixpath.dirname(parent)
    out[ROOT] = entry(ROOT, tarfile.DIRTYPE, 0o755, ids)
    out[WORK] = entry(WORK, tarfile.DIRTYPE, 0o755, ids)
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w", format=tarfile.PAX_FORMAT) as t:
        for name in sorted(out, key=lambda n: n.split("/")):
            m, data = out[name]
            t.addfile(m, io.BytesIO(data) if data is not None else None)
    return buf.getvalue()


def diff_id(layer: bytes) -> str:
    return "sha256:" + hashlib.sha256(layer).hexdigest()


def self_test() -> int:
    bad = []
    tgz_buf = io.BytesIO()
    with tarfile.open(fileobj=tgz_buf, mode="w:gz") as t:
        def add(name, kind=tarfile.REGTYPE, data=b"", mode=0o644, link=""):
            ti = tarfile.TarInfo(name)
            ti.type, ti.mode, ti.linkname, ti.size, ti.mtime, ti.uid = kind, mode, link, len(data), 1234, 500
            t.addfile(ti, io.BytesIO(data) if kind == tarfile.REGTYPE else None)
        add("./", tarfile.DIRTYPE, mode=0o775)
        add("./run.sh", data=b"#!/bin/bash\n", mode=0o755)
        add("./bin", tarfile.DIRTYPE, mode=0o755)
        add("./bin/Runner.Listener", data=b"\x7fELF", mode=0o755)
        add("./externals/node24/bin/node", data=b"\x7fELF", mode=0o755)
        add("./externals/node24/lib/npm-cli.js", data=b"//\n")
        add("./externals/node24/bin/npm", tarfile.SYMTYPE, link="../lib/npm-cli.js", mode=0o777)
    tgz = tgz_buf.getvalue()
    toml = f'version = "9.9.9"\n[sha256]\nx86_64 = "{hashlib.sha256(tgz).hexdigest()}"\naarch64 = "{"0" * 64}"\n'
    environment = ('PATH="/home/runner/.local/bin:/usr/bin:/bin"\n# a comment\n\nImageOS=ubuntu24\n'
                   "ImageVersion=20261111.370.1\nRUNNER_TOOL_CACHE=/opt/hostedtoolcache\n"
                   "AGENT_TOOLSDIRECTORY=/opt/hostedtoolcache\nXDG_CONFIG_HOME='/home/runner/.config'\n")
    passwd = "root:x:0:0:root:/root:/bin/bash\nrunner:x:1001:118:,,,:/home/runner:/bin/bash\n"
    args = (toml, "x86_64", tgz, environment, passwd, b"#!/bin/sh\nhook\n", b"#!/bin/sh\nstarted\n", b"MIT\n")

    def fails(why, *a, want=""):
        try:
            make(*a)
            bad.append(f"{why}: taken")
        except ValueError as e:
            if want not in str(e):
                bad.append(f"{why}: {e}")

    layer = make(*args)
    if make(*args) != layer:
        bad.append("the same inputs give two layers")
    with tarfile.open(fileobj=io.BytesIO(layer)) as t:
        ms = {m.name: m for m in t.getmembers()}
        order = [m.name for m in t.getmembers()]
        text = {n: t.extractfile(m).read().decode() for n, m in ms.items() if m.isreg()}
    r = ROOT + "/"
    for name in (ROOT, WORK, r + "run.sh", r + "bin/Runner.Listener", r + "externals", r + "externals/node24",
                 r + "externals/node24/bin/npm", r + ".env", r + ".path", r + HOOK, r + JOB_STARTED, r + LICENSE):
        if name not in ms:
            bad.append(f"no {name} in the layer")
            ms[name] = tarfile.TarInfo(name)
    for name in ("home", "home/runner"):
        if name in ms:
            bad.append(f"the layer holds {name}: the snapshot owns it")
    if any((m.uid, m.gid, m.mtime) != (1001, 118, 0) or m.uname for m in ms.values()):
        bad.append("an owner or a time other than runner (1001:118) and 0")
    if [n for n in order if not n.startswith(ROOT) and n != WORK]:
        bad.append(f"a path out of the runner root and the work folder: {order}")
    if any(order.index(posixpath.dirname(n)) > i for i, n in enumerate(order) if posixpath.dirname(n) in ms):
        bad.append("a child before its parent")
    if not (ms[WORK].isdir() and ms[WORK].mode == 0o755):
        bad.append(f"the work folder: {ms[WORK].type} {oct(ms[WORK].mode)}")
    if ms[ROOT].mode != 0o755 or ms[r + "run.sh"].mode != 0o755 or ms[r + JOB_STARTED].mode != 0o755:
        bad.append("the mode of the root, run.sh or the hook")
    if ms[r + "externals/node24/bin/npm"].linkname != "../lib/npm-cli.js":
        bad.append("the link of the tarball changed")
    for f in LINKS:
        m = ms.get(r + f)
        to = m and posixpath.normpath(posixpath.join(posixpath.dirname("/" + m.name), m.linkname))
        if not (m and m.issym() and not m.linkname.startswith("/") and to == f"{CREDENTIALS}/{f}"):
            bad.append(f"the link {f}: {m and m.linkname}")
    want_env = ["PATH=/home/runner/.local/bin:/usr/bin:/bin", "ImageOS=ubuntu24", "ImageVersion=20261111.370.1",
                "RUNNER_TOOL_CACHE=/opt/hostedtoolcache", "AGENT_TOOLSDIRECTORY=/opt/hostedtoolcache",
                "XDG_CONFIG_HOME=/home/runner/.config",
                f"ACTIONS_RUNNER_HOOK_JOB_STARTED=/{ROOT}/{JOB_STARTED}"]
    if text.get(r + ".env", "").splitlines() != want_env:
        bad.append(f"the .env: {text.get(r + '.env')!r}")
    if text.get(r + ".path") != "/home/runner/.local/bin:/usr/bin:/bin\n":
        bad.append(f"the .path: {text.get(r + '.path')!r}")
    if text.get(r + JOB_STARTED) != "#!/bin/sh\nstarted\n" or text.get(r + HOOK) != "#!/bin/sh\nhook\n":
        bad.append("the hooks")
    if not diff_id(layer).startswith("sha256:") or len(diff_id(layer)) != 71:
        bad.append("the diff id")
    if pin(toml, "arm64")[2] != ("https://github.com/actions/runner/releases/download/v9.9.9/"
                                 "actions-runner-linux-arm64-9.9.9.tar.gz"):
        bad.append(f"the URL of arm64: {pin(toml, 'arm64')[2]}")

    fails("a tarball of another digest", toml, "aarch64", *args[2:], want="not 000")
    fails("no RUNNER_TOOL_CACHE", *args[:3], environment.replace("RUNNER_TOOL_CACHE", "X"), *args[4:],
          want="no RUNNER_TOOL_CACHE")
    fails("a line that is not KEY=VALUE", *args[:3], environment + "SECRET value\n", *args[4:], want="line 9")
    fails("a hook variable of the image", *args[:3], environment + "ACTIONS_RUNNER_HOOK_JOB_STARTED=/x\n",
          *args[4:], want="the layer sets it")
    fails("no user runner", *args[:4], "root:x:0:0:root:/root:/bin/bash\n", *args[5:], want="no user runner")
    fails("another home", *args[:4], passwd.replace("/home/runner:", "/srv/runner:"), *args[5:], want="home")
    try:
        bad_line = "SECRET-VALUE here\n"
        parse_environment(environment + bad_line)
    except ValueError as e:
        if "SECRET" in str(e):
            bad.append("an error of /etc/environment names its content")

    for why, name, kind, link, want in (
            ("a member out of the root", "../evil", tarfile.REGTYPE, "", "out of the runner root"),
            ("an absolute link", "./x", tarfile.SYMTYPE, "/etc/shadow", "points out"),
            ("a link out of the root", "./bin/x", tarfile.SYMTYPE, "../../../etc", "points out"),
            ("a device", "./dev", tarfile.CHRTYPE, "", "not a file"),
            ("a file of the layer", "./.env", tarfile.REGTYPE, "", "a path of this layer")):
        buf = io.BytesIO()
        with tarfile.open(fileobj=buf, mode="w:gz") as t:
            ti = tarfile.TarInfo(name)
            ti.type, ti.linkname = kind, link
            t.addfile(ti, io.BytesIO(b"") if kind == tarfile.REGTYPE else None)
        evil = buf.getvalue()
        etoml = f'version = "9.9.9"\n[sha256]\nx86_64 = "{hashlib.sha256(evil).hexdigest()}"\naarch64 = ""\n'
        fails(why, etoml, "x86_64", evil, *args[3:], want=want)
    for b in bad:
        print(f"self-test: {b}")
    if not bad:
        print("runner-layer.py: self-test passes")
    return 1 if bad else 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    ap = argparse.ArgumentParser(description="The runner layer of the hosted snapshot (T-541).")
    ap.add_argument("verb", choices=("url", "make"))
    ap.add_argument("--runner-toml", required=True)
    ap.add_argument("--arch", required=True)
    for k in ("tarball", "environment", "passwd", "hook", "job-started", "license", "out"):
        ap.add_argument(f"--{k}")
    a = ap.parse_args()
    toml = open(a.runner_toml, encoding="utf-8").read()
    if a.verb == "url":
        print(pin(toml, a.arch)[2])
        return 0
    need = [k for k in ("tarball", "environment", "passwd", "hook", "job_started", "license", "out")
            if getattr(a, k) is None]
    if need:
        ap.error("make needs " + ", ".join("--" + k.replace("_", "-") for k in need))

    def raw(p):
        return open(p, "rb").read()

    try:
        layer = make(toml, a.arch, raw(a.tarball), raw(a.environment).decode(), raw(a.passwd).decode(),
                     raw(a.hook), raw(a.job_started), raw(a.license))
    except ValueError as e:
        print(f"runner-layer: {e}", file=sys.stderr)
        return 1
    with open(a.out, "wb") as f:
        f.write(layer)
    print(diff_id(layer))
    return 0


if __name__ == "__main__":
    sys.exit(main())
