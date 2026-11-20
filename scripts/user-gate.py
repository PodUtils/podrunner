#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The user-path gate (T-243, T-244): the files of a release, installed and
used as a user, on one host class (a lane of user-gate.yml).

  user-gate.py check
      check the corpus table and the sections of docs/install.md that a
      lane runs
  user-gate.py doc-commands FILE SECTION [--set OLD=NEW]...
      print the commands of the sh blocks of a section of a document
  user-gate.py lane --lane NAME --assets DIR --out DIR [--work DIR]
                    [--executors podvm,runner-dir,microvm] [--no-corpus]
      the checks of one lane, as the user that runs it; OUT/results.tsv
      gets one line for each check
  user-gate.py summary [--lanes A,B] DIR
      one Markdown table (check x lane) of each results.tsv under DIR;
      exit 1 when a check failed or a lane has no results

A lane needs python3, git, curl, tar and gzip on the host. It takes no
root: the lab scripts (scripts/lab/) make the user and the host class.
"""

import argparse
import functools
import http.server
import json
import os
import pathlib
import platform
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tarfile
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
TABLE = ROOT / "scripts" / "corpus" / "user-gate.tsv"
INSTALL_DOC = ROOT / "docs" / "install.md"
COLUMNS = ("id", "repo", "sha", "workflow", "job", "matrix", "github", "executor", "lanes", "expect", "flags", "note")
EXECUTORS = ("podvm", "runner-dir", "microvm", "incus")
RELEASE_URL = "https://github.com/PodUtils/podrunner/releases/download/"
# The sections of docs/install.md that a lane runs as they are written.
QUICK_START = "Quick start"
OFFLINE = "The offline bundle"
# A mirror that refuses each connection: an offline install must not fetch.
DEAD_MIRROR = "http://127.0.0.1:9"
ROW_TIMEOUT = 45 * 60
SAMPLE_CI = """on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: test -f README.md
"""



# The executors whose hello fails for a known cause with an entry: an xfail,
# and a pass fails the gate until the entry goes.
HELLO_XFAIL: dict[str, str] = {}

class GateError(Exception):
    pass


def tcp_listeners_of(pid: int) -> list[str]:
    """The TCP sockets in LISTEN that the process PID holds, as ADDR:PORT
    in hex of /proc/net/tcp and tcp6."""
    listen = {}
    for name in ("tcp", "tcp6"):
        try:
            rows = pathlib.Path(f"/proc/net/{name}").read_text().splitlines()[1:]
        except OSError:
            continue
        for row in rows:
            f = row.split()
            if len(f) > 9 and f[3] == "0A":
                listen[f[9]] = f[1]
    held = []
    for fd in pathlib.Path(f"/proc/{pid}/fd").glob("*"):
        try:
            target = os.readlink(fd)
        except OSError:
            continue
        m = re.fullmatch(r"socket:\[(\d+)\]", target)
        if m and m.group(1) in listen:
            held.append(listen[m.group(1)])
    return held


def read_table(path: pathlib.Path) -> tuple[list[dict], list[str]]:
    rows, errors, seen = [], [], set()
    for n, line in enumerate(path.read_text().splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        where = f"{path.name}:{n}"
        f = line.split("\t")
        if len(f) != len(COLUMNS):
            errors.append(f"{where}: {len(f)} columns, not {len(COLUMNS)}")
            continue
        r = dict(zip(COLUMNS, f))
        # `xfail userns-on:ok`: the expect of each lane, else the first word
        # (a rung of podvm can pass a row that the others fail, T-564).
        words = r["expect"].split(" ")
        r["expect"], r["expect_by_lane"] = words[0], {}
        for w in words[1:]:
            lane, _, value = w.partition(":")
            if not lane or value not in ("ok", "xfail", "flaky"):
                errors.append(f"{where}: a lane of expect is LANE:ok, LANE:xfail or LANE:flaky, not {w!r}")
            r["expect_by_lane"][lane] = value
        if not re.fullmatch(r"[0-9a-f]{40}", r["sha"]):
            errors.append(f"{where}: the sha is not 40 hex digits")
        if not re.fullmatch(r"[\w.-]+/[\w.-]+", r["repo"]):
            errors.append(f"{where}: the repo is not OWNER/REPO")
        if r["executor"] not in EXECUTORS:
            errors.append(f"{where}: no executor {r['executor']!r}")
        if r["github"] != "success":
            errors.append(f"{where}: the corpus takes jobs that pass on github.com, not {r['github']!r}")
        if r["expect"] not in ("ok", "xfail", "flaky"):
            errors.append(f"{where}: expect is ok, xfail or flaky, not {r['expect']!r}")
        if r["expect"] in ("xfail", "flaky") and not re.search(r"\bT-\d{3,}\b", r["note"]):
            errors.append(f"{where}: an {r['expect']} row names the entry of its cause in the note")
        if r["matrix"] != "-" and not all(re.fullmatch(r"[\w-]+=\S+", kv) for kv in r["matrix"].split(";")):
            errors.append(f"{where}: the matrix is not KEY=VALUE;KEY=VALUE")
        if (r["id"], r["executor"]) in seen:
            errors.append(f"{where}: {r['id']} on {r['executor']} twice")
        seen.add((r["id"], r["executor"]))
        rows.append(r)
    if not rows:
        errors.append(f"{path.name}: no rows")
    return rows, errors


def doc_commands(path: pathlib.Path, section: str, sets: list[tuple[str, str]]) -> str:
    """The lines of the sh blocks under the heading SECTION, with each OLD
    replaced by NEW."""
    out, inside, level, fence, found = [], False, 0, None, False
    for line in path.read_text().splitlines():
        if fence is None:
            h = re.match(r"(#+)\s+(.*?)\s*$", line)
            if h:
                if inside and len(h.group(1)) <= level:
                    break
                if h.group(2) == section:
                    inside, level, found = True, len(h.group(1)), True
                continue
            if not inside:
                continue
            f = re.match(r"(```+)\s*(\w*)", line)
            if f:
                fence = (f.group(1), f.group(2) in ("sh", "shell", "bash"))
        elif line.strip() == fence[0]:
            fence = None
        elif fence[1]:
            out.append(line)
    if not found:
        raise GateError(f"{path.name}: no section {section!r}")
    if not out:
        raise GateError(f"{path.name}: the section {section!r} has no sh block")
    text = "\n".join(out) + "\n"
    for old, new in sets:
        text = text.replace(old, new)
    return text


def read_pins(installer: pathlib.Path) -> dict[str, str]:
    text = installer.read_text()
    block = text[text.index("# BEGIN PINS") : text.index("# END PINS")]
    return dict(re.findall(r'^(\w+)="(.*)"$', block, re.M))


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


class Lane:
    def __init__(self, a):
        self.name = a.lane
        self.assets = pathlib.Path(a.assets).resolve()
        self.work = pathlib.Path(a.work or pathlib.Path.home() / "user-gate").resolve()
        self.out = pathlib.Path(a.out).resolve()
        self.executors = [e for e in a.executors.split(",") if e]
        for e in self.executors:
            if e not in EXECUTORS:
                raise SystemExit(f"no executor {e!r}")
        self.results: list[tuple[str, str, str]] = []
        self.serve_kinds = [k for k in (a.serve_kinds or "").split(",") if k]
        # The rung that the class of the lane must give podvm (T-564).
        self.want_rung = getattr(a, "want_rung", "") or ""
        # A Python with jsonschema for the check of podrunner.toml (T-507).
        self.schema_python = getattr(a, "schema_python", "") or ""
        for k in self.serve_kinds:
            if k not in ("incus", "microvm", "podvm"):
                raise SystemExit(f"no serve kind {k!r}")
        pins = read_pins(self.assets / "podrunner-install.sh")
        self.version, self.runner_version = pins["PODRUNNER_VERSION"], pins["RUNNER_VERSION"]
        arch = {"x86_64": "x86_64", "amd64": "x86_64", "aarch64": "aarch64", "arm64": "aarch64"}
        self.arch = arch[platform.machine()]
        if self.work.exists():
            shutil.rmtree(self.work)
        self.work.mkdir(parents=True)
        (self.out / "logs").mkdir(parents=True, exist_ok=True)
        self.home = self.work / "home"
        self.data = self.home / ".local" / "share" / "podrunner"
        self.image = ""
        # The home of the lane as a home is: no write of the group or the
        # others, whatever the umask of the lab user (podrunner refuses a
        # driver under an open directory, T-537).
        self.home.mkdir()
        for d in (self.work, self.home):
            d.chmod(0o755)

    # The helpers.
    def env(self, home: pathlib.Path, **extra) -> dict[str, str]:
        e = {k: v for k, v in os.environ.items() if not k.startswith("XDG_") and k != "PODRUNNER_MIRROR"}
        # A login shell of Ubuntu puts ~/.local/bin into PATH when it exists.
        e["HOME"], e["PATH"] = str(home), f"{home}/.local/bin:" + os.environ.get("PATH", "/usr/bin:/bin")
        e.update(extra)
        return e

    def run(self, argv, env, cwd, log: str, timeout=None) -> tuple[int, str]:
        """Run ARGV with its stdout in logs/LOG.out and its stderr in
        logs/LOG.err. The descriptors of the lane stay open in it
        (inherited-fds). Return the exit code and the stdout."""
        so, se = self.out / "logs" / f"{log}.out", self.out / "logs" / f"{log}.err"
        argv = [str(x) for x in argv]
        with so.open("w") as o, se.open("w") as e:
            e.write(f"user-gate: in {cwd}: {shlex.join(argv)}\n")
            e.flush()
            p = subprocess.Popen(argv, cwd=cwd, env=env, stdout=o, stderr=e, close_fds=False, start_new_session=True)
            try:
                rc = p.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                os.killpg(p.pid, signal.SIGTERM)
                try:
                    p.wait(timeout=60)
                except subprocess.TimeoutExpired:
                    os.killpg(p.pid, signal.SIGKILL)
                    p.wait()
                e.write(f"\nuser-gate: stopped after {timeout} s\n")
                rc = 124
        return rc, so.read_text(errors="replace")

    def show(self, log: str, lines: int = 300):
        for ext in ("out", "err"):
            path = self.out / "logs" / f"{log}.{ext}"
            if not path.is_file():
                continue
            text = path.read_text(errors="replace").splitlines()
            print(f"---- {log}.{ext} (the last {min(lines, len(text))} of {len(text)} lines)")
            print("\n".join(text[-lines:]))

    def check(self, name: str, fn) -> bool:
        print(f"==== {self.name}: {name}", flush=True)
        try:
            got = fn()
            result, detail = got if isinstance(got, tuple) else ("pass", got)
        except (GateError, OSError, subprocess.SubprocessError) as e:
            result, detail = "fail", str(e)
        print(f"{result} {name}: {detail}", flush=True)
        self.results.append((name, result, detail.replace("\t", " ").replace("\n", " ")))
        return result in ("pass", "xfail")

    def skip(self, name: str, why: str):
        print(f"skip {name}: {why}", flush=True)
        self.results.append((name, "skip", why))

    # The checks.
    def serve(self) -> str:
        """A local HTTP server with the assets at the paths of the release."""
        www = self.work / "www"
        rel = www / "download" / f"v{self.version}"
        rel.mkdir(parents=True)
        for f in self.assets.iterdir():
            (rel / f.name).symlink_to(f)
        # podvm, podnet and the runner tarball come from their own releases;
        # the bundle holds the same files with the same pins.
        src = self.work / "bundle-src"
        with tarfile.open(self.assets / f"podrunner-bundle-{self.arch}.tar.gz") as t:
            t.extractall(src, filter="data")
        for f in src.glob("*/*"):
            if not (rel / f.name).exists():
                (rel / f.name).symlink_to(f)
        srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Quiet, directory=str(www)))
        threading.Thread(target=srv.serve_forever, daemon=True).start()
        self.base = f"http://127.0.0.1:{srv.server_port}"
        self.mirror = f"{self.base}/download/v{self.version}"
        return f"the assets of v{self.version} at {self.mirror}"

    def sample_project(self) -> pathlib.Path:
        p = self.work / "project"
        (p / ".github" / "workflows").mkdir(parents=True)
        (p / "README.md").write_text("A project of the user-path gate.\n")
        (p / ".github" / "workflows" / "ci.yml").write_text(SAMPLE_CI)
        git = ["git", "-C", str(p), "-c", "user.name=user-gate", "-c", "user.email=user-gate@example.invalid"]
        for argv in (
            git + ["init", "-q", "-b", "main"],
            git + ["remote", "add", "origin", "https://github.com/example/project.git"],
            git + ["add", "-A"],
            git + ["commit", "-q", "-m", "A project"],
        ):
            subprocess.run(argv, check=True, stdout=subprocess.DEVNULL)
        return p

    def doc_script(self, section: str, sets, cwd: pathlib.Path, env, log: str) -> str:
        script = doc_commands(INSTALL_DOC, section, sets)
        path = self.out / "logs" / f"{log}.sh"
        path.write_text(script)
        rc, out = self.run(["sh", "-ex", path], env, cwd, log)
        if rc:
            self.show(log)
            raise GateError(f"exit {rc}: a command of docs/install.md, {section}, failed ({path.name})")
        return out

    def quick_start(self) -> str:
        home, cwd = self.work / "home-docs", self.work / "docs-cwd"
        home.mkdir()
        cwd.mkdir()
        sets = [(RELEASE_URL, f"{self.base}/download/"), ("YOUR-PROJECT", str(self.sample_project())), ("VERSION", self.version)]
        # The documented PODRUNNER_MIRROR takes the downloads of the
        # installer to the local server in place of github.com.
        self.doc_script(QUICK_START, sets, cwd, self.env(home, PODRUNNER_MIRROR=self.mirror), "docs-quick-start")
        return "each command of docs/install.md, Quick start, passed (online, from the local server)"

    def install_bundle(self) -> str:
        dl = self.work / "download"
        dl.mkdir()
        bundle = f"podrunner-bundle-{self.arch}.tar.gz"
        for n in ("podrunner-install.sh", bundle):
            shutil.copy(self.assets / n, dl / n)
        env = self.env(self.home, PODRUNNER_MIRROR=DEAD_MIRROR)
        rc, out = self.run(["sh", "podrunner-install.sh", "--bundle", bundle], env, dl, "install-bundle")
        if rc:
            self.show("install-bundle")
            raise GateError(f"sh podrunner-install.sh --bundle {bundle}: exit {rc}")
        if "podrunner-install: fetch" in out:
            raise GateError("the install from the bundle fetched a file")
        return f"sh podrunner-install.sh --bundle {bundle}, with no network"

    def install_offline_doc(self) -> str:
        env = self.env(self.home, PODRUNNER_MIRROR=DEAD_MIRROR)
        out = self.doc_script(OFFLINE, [("x86_64", self.arch)], self.work / "download", env, "install-offline-doc")
        if "nothing was downloaded" not in out:
            raise GateError("the second install did not say 'nothing was downloaded'")
        return "each command of docs/install.md, The offline bundle, passed; a second install takes nothing"

    def install_full(self) -> str:
        full = self.assets / f"runner-full-{self.arch}.tar"
        if not full.is_file():
            raise GateError(f"no {full.name} in the assets")
        bdir = self.work / "download" / f"podrunner-bundle-{self.version}-{self.arch}"
        shutil.copy(full, bdir / full.name)
        before = self.run_toml_image()
        env = self.env(self.home, PODRUNNER_MIRROR=DEAD_MIRROR)
        rc, _ = self.run(["sh", bdir / "podrunner-install.sh", "--full"], env, self.work / "download", "install-full")
        if rc:
            self.show("install-full")
            raise GateError(f"podrunner-install.sh --full: exit {rc}")
        if self.run_toml_image() == before:
            raise GateError("run.toml names the base image after --full")
        return "the larger image from the bundle directory (--full)"

    def run_toml_image(self) -> str:
        toml = self.home / ".config" / "podrunner" / "run.toml"
        m = re.search(r'^image = "(.*)"$', toml.read_text(), re.M)
        if not m:
            raise GateError(f"{toml}: no image")
        self.image = m.group(1)
        return self.image

    def flags(self, executor: str) -> list[str]:
        if executor == "podvm":
            return []  # the run.toml of the installer
        if executor == "runner-dir":
            return ["--no-config", "--runner-dir", str(self.data / "runner" / self.runner_version)]
        # A guest of podvm's machine tier, or a system container of its incus
        # tier (incusd, T-123, T-528).
        guest = "--incus" if executor == "incus" else "--microvm"
        return ["--no-config", guest, str(self.data / "bin" / "podvm"), "--images", str(self.data / "images"), "--image", self.image]

    def podrunner(self, args, cwd, log, expect_ok: bool, timeout=None) -> tuple[bool, str]:
        """doctor --run, then run, with ARGS. True when each job is ok."""
        env = self.env(self.home)
        rc, _ = self.run(["podrunner", "doctor", "--run", *args], env, cwd, f"{log}-doctor")
        if rc:
            if expect_ok:
                self.show(f"{log}-doctor")
            return False, f"podrunner doctor --run: exit {rc}"
        rc, out = self.run(["podrunner", "run", *args, "--log-dir", self.out / "logs" / log], env, cwd, log, timeout)
        lines = [line for line in out.splitlines() if line.strip()]
        ok = rc == 0 and any(line.startswith("ok ") for line in lines)
        if not ok and expect_ok:
            self.show(log, 100)
            for job in sorted((self.out / "logs" / log).glob("*/job.log")):
                text = job.read_text(errors="replace").splitlines()
                print(f"---- {job.parent.name}/job.log (the last {min(400, len(text))} lines)")
                print("\n".join(text[-400:]))
        if not lines:
            # A refusal before the first job is on stderr only (T-253).
            err = (self.out / "logs" / f"{log}.err").read_text(errors="replace").splitlines()
            lines = [line for line in err if line.startswith("podrunner:")][-2:]
        return ok, f"podrunner run: exit {rc}: " + "; ".join(lines[-5:])

    def hello(self, executor: str):
        ok, detail = self.podrunner([self.data / "hello.yml", *self.flags(executor)], self.work, f"hello-{executor}", True)
        known = HELLO_XFAIL.get(executor)
        if executor == "microvm" and not ok:
            detail += " | probe: " + self.microvm_probe()
        if known and ok:
            raise GateError(f"hello on {executor} passes now (an xfail of HELLO_XFAIL): remove it")
        if known:
            return "xfail", f"{known} ({detail})"
        if not ok:
            raise GateError(detail)
        return detail

    def confine(self, executor: str):
        """A static binary of a job (Go 1.20 makes raw system calls) writes
        an absolute path of the home of the user: the file must not come on
        the host (the third QA, finding A1, T-300)."""
        mark = pathlib.Path.home() / "podrunner-gate-escaped"
        mark.unlink(missing_ok=True)
        src = self.work / "confine"
        shutil.rmtree(src, ignore_errors=True)
        (src / ".github" / "workflows").mkdir(parents=True)
        (src / "esc.go").write_text(
            'package main\n\nimport "os"\n\nfunc main() {\n'
            f'\tos.WriteFile("{mark}", []byte("a job wrote this\\n"), 0o644)\n}}\n'
        )
        (src / ".github" / "workflows" / "esc.yml").write_text(
            "on: push\njobs:\n  esc:\n    runs-on: ubuntu-latest\n    steps:\n"
            "      - uses: actions/checkout@v4\n"
            "      - uses: actions/setup-go@v5\n        with:\n          go-version: '1.20'\n          cache: false\n"
            "      - run: echo 'module esc' > go.mod && GOCACHE=/tmp/gocache go build -o /tmp/esc . && /tmp/esc\n"
        )
        git = ["git", "-C", str(src), "-c", "user.name=gate", "-c", "user.email=gate@example.invalid"]
        subprocess.run(["git", "init", "-q", str(src)], check=True)
        subprocess.run([*git, "add", "-A"], check=True)
        subprocess.run([*git, "commit", "-qm", "confine"], check=True)
        ok, detail = self.podrunner([".github/workflows/esc.yml", "--checkout", ".", *self.flags(executor)], src, f"confine-{executor}", False)
        if mark.exists():
            mark.unlink()
            raise GateError(f"a job in a unit wrote {mark} on the host ({detail})")
        # On the loader family Go finds no GOROOT and the job fails: the
        # check holds on the file, and names the result of the job.
        return f"{mark} is not on the host (the job: {'ok' if ok else detail})"

    def pids(self, executor: str):
        """A step must not see or signal a process of the user on the host
        (the fourth QA, finding F1, T-318). Only the namespace rung of
        podvm 0.2.0-beta.30 or later gives a unit a PID namespace: on
        another rung the check is an xfail that names the rung."""
        src = self.work / "pids"
        shutil.rmtree(src, ignore_errors=True)
        (src / ".github" / "workflows").mkdir(parents=True)
        (src / ".github" / "workflows" / "pids.yml").write_text(
            "on: push\njobs:\n  pids:\n    runs-on: ubuntu-latest\n    steps:\n"
            "      - run: |\n"
            # The text of the step holds no answer: the log echoes the step.
            "          r=seen; kill -0 \"$HOSTPID\" 2>/dev/null || r=\"not $r\"; echo \"host pid: $r\"\n"
        )
        host = subprocess.Popen(["sleep", "600"], start_new_session=True)
        try:
            wf = [".github/workflows/pids.yml", "--env", f"HOSTPID={host.pid}", *self.flags(executor)]
            ok, detail = self.podrunner(wf, src, f"pids-{executor}", True)
        finally:
            host.kill()
            host.wait()
        logs = "".join(j.read_text(errors="replace") for j in (self.out / "logs" / f"pids-{executor}").glob("*/job.log"))
        said = re.findall(r"host pid: (not seen|seen)\s*$", logs, re.M)
        if not ok or not said:
            raise GateError(f"the job did not say what it sees ({detail})")
        doctor = (self.out / "logs" / f"pids-{executor}-doctor.out").read_text(errors="replace")
        m = re.search(r"rung: (\S+?):", doctor)
        rung = m.group(1) if m else "unknown"
        if said[-1] == "not seen":
            return f"a step does not see a process of the host (rung {rung})"
        if rung == "namespace":
            raise GateError("a step on the namespace rung sees a process of the host (T-318)")
        return "xfail", f"rung {rung}: a unit has no PID namespace there; a step sees the processes of the user"

    def pty(self):
        """A step opens a pty in a podvm unit (a user QA, finding F3, T-354).
        podvm 0.2.0-beta.32 or later mounts a devpts of the unit on the
        namespace rung: on another rung a failure is an xfail that names the
        rung."""
        src = self.work / "pty"
        shutil.rmtree(src, ignore_errors=True)
        (src / ".github" / "workflows").mkdir(parents=True)
        (src / ".github" / "workflows" / "pty.yml").write_text(
            "on: push\njobs:\n  pty:\n    runs-on: ubuntu-latest\n    steps:\n"
            "      - run: |\n"
            # The text of the step holds no answer: the log echoes the step.
            "          command -v script >/dev/null || { echo \"a pty: no tool\"; exit 0; }\n"
            "          r=open; script -qec 'echo x' /dev/null >/dev/null 2>&1 || r=\"not $r\"; echo \"a pty: $r\"\n"
        )
        ok, detail = self.podrunner([".github/workflows/pty.yml", *self.flags("podvm")], src, "pty-podvm", True)
        logs = "".join(j.read_text(errors="replace") for j in (self.out / "logs" / "pty-podvm").glob("*/job.log"))
        said = re.findall(r"a pty: (no tool|not open|open)\s*$", logs, re.M)
        if not ok or not said:
            raise GateError(f"the job did not say if it has a pty ({detail})")
        if said[-1] == "no tool":
            return "xfail", "the image of the unit has no `script`"
        doctor = (self.out / "logs" / "pty-podvm-doctor.out").read_text(errors="replace")
        m = re.search(r"rung: (\S+?):", doctor)
        rung = m.group(1) if m else "unknown"
        if said[-1] == "open":
            return f"a step opens a pty (rung {rung})"
        if rung == "namespace":
            raise GateError("a step on the namespace rung opens no pty (T-354)")
        return "xfail", f"rung {rung}: a unit has no devpts there"

    def hold(self):
        """The egress hold of `run.toml` (its `podnet` and `allow` lines): a
        step reaches an allowed host through the seccomp hold of podnet and
        the egress module, and no other host. The lane userns-off is H0,
        where the runner timed out on github.com in the live run (T-179)."""
        src = self.work / "hold"
        shutil.rmtree(src, ignore_errors=True)
        (src / ".github" / "workflows").mkdir(parents=True)
        (src / ".github" / "workflows" / "hold.yml").write_text(
            "on: push\njobs:\n  hold:\n    runs-on: ubuntu-latest\n    steps:\n"
            "      - run: |\n"
            "          code=$(curl -sS -o /dev/null -w '%{http_code}' --max-time 60 https://github.com/) || true\n"
            "          echo \"allowed host: http $code\"\n"
            "          case $code in 2??|3??) ;; *) exit 1 ;; esac\n"
            "          if curl -sS -o /dev/null --max-time 15 https://example.com/; then echo 'other host: reached'; exit 1; fi\n"
            "          echo 'other host: refused'\n"
        )
        hold = ["--podnet", str(self.data / "bin" / "podnet"), "--allow", "github.com"]
        ok, detail = self.podrunner([".github/workflows/hold.yml", *self.flags("podvm"), *hold], src, "hold-podvm", True, timeout=900)
        if not ok:
            raise GateError(f"a job under --podnet --allow github.com: {detail}")
        return "under --podnet, the job reached github.com and not example.com"

    def error_paths(self):
        """T-372: runs whose way out or input fails. Each must end within a
        limit, with the documented exit code, and with a line that names the
        cause and a fix. A measure until each case holds: a case that does
        not hold is named in an xfail."""
        src = self.work / "errors"
        shutil.rmtree(src, ignore_errors=True)
        (src / ".github" / "workflows").mkdir(parents=True)
        (src / ".github" / "workflows" / "fetch.yml").write_text(
            "on: push\njobs:\n  f:\n    runs-on: ubuntu-latest\n    steps:\n"
            "      - uses: actions/upload-artifact@v4\n        with: {name: a, path: fetch.yml}\n"
        )
        (src / ".github" / "workflows" / "plain.yml").write_text(
            "on: push\njobs:\n  p:\n    runs-on: ubuntu-latest\n    steps: [{run: 'true'}]\n"
        )
        dead = "http://127.0.0.1:9"
        zero = "runner@sha256:" + "0" * 64
        cases = [
            # (name, argv, extra environment, exit code, words of the last line)
            ("dead-proxy", [".github/workflows/fetch.yml", *self.flags("runner-dir"), "--actions", self.work / "errors-actions"],
             {"HTTPS_PROXY": dead, "https_proxy": dead, "HTTP_PROXY": dead, "http_proxy": dead}, 78, ["proxy", "--local-repository"]),
        ]
        if "podvm" in self.executors:
            cases += [
                ("no-toolcache", [".github/workflows/plain.yml", "--toolcache", self.work / "no-such-toolcache"], {}, 78, ["--toolcache", "mkdir -p"]),
                ("no-image", [".github/workflows/plain.yml", "--image", zero], {}, 78, ["store"]),
            ]
        failed = []
        for name, args, extra, code, words in cases:
            env = self.env(self.home, **extra)
            started = time.monotonic()
            rc, _ = self.run(["podrunner", "run", *args], env, src, f"error-{name}", timeout=180)
            took = time.monotonic() - started
            err = (self.out / "logs" / f"error-{name}.err").read_text(errors="replace")
            last = ([line for line in err.splitlines() if line.startswith("podrunner:")] or [""])[-1]
            missing = [w for w in words if w not in last]
            if rc != code or took > 120 or missing:
                failed.append(f"{name}: exit {rc} (want {code}), {took:.0f} s, the last line {last[:160]!r} lacks {missing}")
        if failed:
            return "xfail", "T-372: " + " | ".join(failed)
        return f"{len(cases)} error paths end in time, with their code and a fix"

    def rung(self):
        """A measure for podvm (PodUtils/podvm#466, #470): the rung and the
        mode of a detached unit of the runner image with a volume, as a
        unit of podrunner has (T-261), and whether it gives a pty and has
        `less` (T-353, T-354). It fails only when no unit starts."""
        podvm = self.data / "bin" / "podvm"
        env = self.env(self.home, PODVM_STORE=str(self.data / "images"))
        sock = self.work / "rung-sock"
        sock.mkdir(parents=True, exist_ok=True)
        name = f"gate-rung-{os.getpid()}"
        argv = [podvm, "run", "-d", "--name", name, "-v", f"{sock}:/run/podrunner", self.image, "sleep", "120"]
        rc, _ = self.run(argv, env, self.work, "rung-run", timeout=600)
        if rc:
            self.show("rung-run")
            raise GateError(f"podvm run -d of the runner image: exit {rc}")
        try:
            _, rung = self.run([podvm, "inspect", name, "--format", "{{.Rung}} {{.Exec.Mode}}"], env, self.work, "rung-inspect")
            probe = "python3 -c 'import os; os.openpty()' >/dev/null 2>&1 && echo pty=yes || echo pty=no; command -v less >/dev/null && echo less=yes || echo less=no"
            _, tools = self.run([podvm, "exec", name, "sh", "-c", probe], env, self.work, "rung-exec")
        finally:
            self.run([podvm, "rm", "-f", name], env, self.work, "rung-rm")
        words = " ".join(tools.split())
        got = (rung.split() or ["unknown"])[0]
        if self.want_rung and got != self.want_rung:
            raise GateError(f"the lane needs the {self.want_rung} rung of podvm, and its unit took {rung.strip() or 'none'}")
        detail = f"rung of a unit with a volume: {rung.strip() or 'unknown'}; {words}"
        return ("xfail", f"{detail} (T-353, T-354)") if "=no" in words else detail

    # The checks of `serve` (T-527): the host class of githome's pool, with
    # incusd and KVM, as a user with no root.
    def serve_config(self, kind: str) -> pathlib.Path:
        """`config init` for KIND: a program source (the fake source of the
        release) at the tier that KIND serves; the microvm kind also gets
        the pin of a sandbox (T-506) and one sandbox offer."""
        d = self.work / f"serve-{kind}"
        d.mkdir(parents=True, exist_ok=True)
        tier = "untrusted" if kind == "microvm" else "trusted"
        script = d / "script"
        offers = []
        if kind == "microvm":
            offer = {
                "schema": 1, "job_id": "gate-sandbox", "source": "gate", "labels": [], "arch": self.arch,
                "wait_deadline_s": 900,
                "trust_inputs": {
                    "event": "pull_request", "head_is_fork": "yes", "actor_class": "outside",
                    "repo_visibility": "public", "owner": "example-org", "repo": "app", "run_id": 1,
                    "ref": "refs/pull/1/merge", "workflow_ref": "w",
                },
                "sandbox": {"image": self.image, "command": ["sh", "-c", "echo podrunner-sandbox-ok"], "max_s": 600},
            }
            offers.append(json.dumps({"at_s": 0, "offer": offer}))
        script.write_text("".join(f"{o}\n" for o in offers))
        built = int(time.time())
        manifest = self.assets / "podrunner-manifest.json"
        if manifest.is_file():
            images = json.loads(manifest.read_text())["runner"]["images"].get(self.arch, {})
            digest = self.image.rpartition("@")[2]
            built = next((i["built_at_s"] for i in images.values() if i and i.get("digest") == digest and i.get("built_at_s")), built)
        out = d / "podrunner.toml"
        argv = [
            "podrunner", "config", "init", "--source", "gate", "--program", shutil.which("podrunner", path=self.env(self.home)["PATH"]),
            "--arg", "fake-source", "--arg", "--script", "--arg", script, "--tier", tier, "--org", "example-org",
            "--podvm", self.data / "bin" / "podvm", "--images", self.data / "images", "--image", self.image,
            "--built-at-s", built, "--kinds", kind, "--host", f"gate-{kind}", "--out", out, "--json",
        ] + self.hold_args(kind)
        rc, text = self.run(argv, self.env(self.home), d, f"serve-{kind}-init")
        if rc:
            self.show(f"serve-{kind}-init")
            raise GateError(f"podrunner config init --kinds {kind}: exit {rc}")
        got = json.loads(text.strip().splitlines()[-1])
        if got.get("kinds") != [kind]:
            raise GateError(f"config init: kinds {got.get('kinds')}, refused {got.get('refused')}")
        if kind == "microvm":
            # The pin of the image becomes the pin of a sandbox (T-506): a
            # second pin of the same image is refused by check.
            text = out.read_text()
            line = f'reference = "{self.image}"\n'
            if line not in text:
                raise GateError(f"config init wrote no pin of {self.image}")
            out.write_text(text.replace(line, line + "sandbox = true\n", 1))
        return out

    def serve_check(self, kind: str) -> str:
        """config init, check, doctor --json (the boundary and the hold of
        the kind) and print-unit."""
        cfg = self.serve_config(kind)
        env = self.env(self.home)
        rc, _ = self.run(["podrunner", "check", "--config", cfg], env, cfg.parent, f"serve-{kind}-check")
        if rc:
            self.show(f"serve-{kind}-check")
            raise GateError(f"podrunner check: exit {rc}")
        rc, text = self.run(["podrunner", "doctor", "--config", cfg, "--json"], env, cfg.parent, f"serve-{kind}-doctor")
        v = json.loads(text)
        cap = next((k.get("capability") for k in v.get("kinds", []) if k.get("kind") == kind), None)
        # doctor runs the self-test of podvm's confine layer, as serve does
        # (T-566): a trusted podvm kind is `confined` when it is proven.
        want = "guest" if kind == "microvm" else "confined"
        if not cap or cap.get("boundary") != want:
            raise GateError(f"doctor --json: the {kind} kind is {cap and cap.get('boundary')}, not {want}: {v.get('kinds')}")
        if not all(src.get("served") for src in v.get("sources", [])):
            raise GateError(f"doctor --json: a source is not served: {v.get('sources')}")
        # T-507, T-479: the file of config init agrees with the schema.
        schema = "schema not checked (no --schema-python)"
        if self.schema_python:
            rc, _ = self.run([self.schema_python, "-I", ROOT / "scripts" / "check-config-schema.py", cfg], env, cfg.parent,
                             f"serve-{kind}-schema")
            if rc:
                self.show(f"serve-{kind}-schema")
                raise GateError(f"the file of config init does not agree with schemas/config.v1.json: exit {rc}")
            schema = "the schema"
        rc, unit = self.run(["podrunner", "print-unit", "--config", cfg, "--state", cfg.parent / "state",
                             "--binary", shutil.which("podrunner", path=env["PATH"])], env, cfg.parent, f"serve-{kind}-unit")
        if rc or "Type=notify" not in unit:
            raise GateError(f"podrunner print-unit: exit {rc}")
        return f"config init, {schema}, check, doctor ({kind}: {want}, hold {cap.get('egress_hold')}), print-unit"

    def serve_run(self, kind: str) -> str:
        """`serve` with the kind: it starts, and the microvm kind runs the
        sandbox offer to a report of `success`."""
        cfg = self.work / f"serve-{kind}" / "podrunner.toml"
        state = cfg.parent / "state"
        env = self.env(self.home)
        env.pop("NOTIFY_SOCKET", None)
        log = self.out / "logs" / f"serve-{kind}.err"
        journal = state / "journal.jsonl"
        with log.open("w") as e:
            p = subprocess.Popen(["podrunner", "serve", "--config", str(cfg), "--state", str(state)], cwd=cfg.parent,
                                 env=env, stdout=subprocess.DEVNULL, stderr=e, start_new_session=True)
            try:
                end = time.monotonic() + (900 if kind == "microvm" else 60)
                reported = None
                while time.monotonic() < end and p.poll() is None:
                    lines = journal.read_text().splitlines() if journal.is_file() else []
                    done = [json.loads(x) for x in lines if '"reported"' in x]
                    if done:
                        reported = done[-1]
                        break
                    time.sleep(2)
                alive = p.poll() is None
                # T-507: serve opens no TCP listener (the host keeps no
                # inbound port; githome's agent comes through podssh).
                listening = tcp_listeners_of(p.pid) if alive else []
            finally:
                if p.poll() is None:
                    os.killpg(p.pid, signal.SIGTERM)
                    try:
                        p.wait(timeout=120)
                    except subprocess.TimeoutExpired:
                        os.killpg(p.pid, signal.SIGKILL)
                        p.wait()
        if listening:
            raise GateError(f"serve listens on TCP: {listening}")
        if kind != "microvm":
            if not alive:
                self.show(f"serve-{kind}", 100)
                raise GateError(f"serve ended: exit {p.returncode}")
            return "serve started and stopped on SIGTERM, with no TCP listener"
        if not reported:
            self.show(f"serve-{kind}", 200)
            raise GateError("no report of the sandbox offer")
        if reported.get("reason") != "success" or reported.get("unit_kind") != kind:
            self.show(f"serve-{kind}", 200)
            raise GateError(f"the sandbox offer: {reported}")
        return f"serve ran the sandbox offer in a unit of {kind}: {reported.get('reason')}"

    # A runner job through `serve` (T-527): githome's source and GitHub as
    # the fakes of podcrates (images/fake-githome.toml).
    def fake(self, name: str) -> pathlib.Path:
        """The pinned binary NAME of podcrates (pod-fake-githome or
        pod-fake-github), downloaded once and checked by its SHA-256."""
        import hashlib
        import tomllib
        import urllib.request
        pins = tomllib.loads((ROOT / "images" / "fake-githome.toml").read_text())
        key = self.arch if name == "pod-fake-githome" else f"github_{self.arch}"
        want = pins["sha256"][key]
        path = self.work / "fakes" / name
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != want:
            path.parent.mkdir(parents=True, exist_ok=True)
            url = (f"https://github.com/PodUtils/podcrates/releases/download/v{pins['version']}/"
                   f"{name}-{self.arch}-unknown-linux-musl")
            body = urllib.request.urlopen(url, timeout=120).read()
            if hashlib.sha256(body).hexdigest() != want:
                raise GateError(f"{url}: the SHA-256 is not the pin of images/fake-githome.toml")
            path.write_bytes(body)
        # serve refuses a program in a directory that others can write.
        path.parent.chmod(0o755)
        path.chmod(0o755)
        return path

    @staticmethod
    def host_address() -> str:
        """An address of this host that is not the loopback: under the
        hold of a unit its loopback is its own, so a broker there is not
        reached (T-527). A UDP connect sends nothing."""
        import socket
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as u:
            u.connect(("192.0.2.1", 9))
            return u.getsockname()[0]

    def started(self, argv, log: str, env) -> tuple[subprocess.Popen, str]:
        """ARGV, which prints its base URL on the first line of stdout."""
        out = (self.out / "logs" / f"{log}.err").open("w")
        p = subprocess.Popen([str(a) for a in argv], stdout=subprocess.PIPE, stderr=out, env=env, text=True,
                             start_new_session=True)
        line = p.stdout.readline().strip()
        if not line.startswith("http://"):
            p.kill()
            raise GateError(f"{argv[0]}: no base URL ({line!r})")
        return p, line.rstrip("/")

    def serve_job(self, kind: str) -> str:
        """One runner job of githome's fake source through `serve` on KIND:
        the claim, the mint of a JIT configuration of the fake GitHub, the
        official runner in a unit of KIND, and the report `success`."""
        import urllib.request
        env = self.env(self.home)
        env.pop("NOTIFY_SOCKET", None)
        tier = "untrusted" if kind == "microvm" else "trusted"
        gh, gh_url = self.started([self.fake("pod-fake-github"), "--listen", f"{self.host_address()}:0"], f"job-{kind}-github", env)
        ho, ho_url = self.started([self.fake("pod-fake-githome"), "serve", "--listen", "127.0.0.1:0", "--github", gh_url],
                                  f"job-{kind}-githome", env)
        d = self.work / f"serve-job-{kind}"
        shutil.rmtree(d, ignore_errors=True)
        d.mkdir(parents=True)
        cfg = d / "podrunner.toml"
        argv = [
            "podrunner", "config", "init", "--source", "githome", "--program", self.fake("pod-fake-githome"),
            "--arg", "source", "--arg", "--server", "--arg", ho_url, "--tier", tier, "--org", "example-org",
            "--podvm", self.data / "bin" / "podvm", "--images", self.data / "images", "--image", self.image,
            "--built-at-s", self.built_at(), "--kinds", kind, "--host", f"gate-job-{kind}", "--out", cfg, "--json",
        ] + self.hold_args(kind)
        procs = [gh, ho]
        try:
            rc, _ = self.run(argv, env, d, f"job-{kind}-init")
            if rc:
                self.show(f"job-{kind}-init")
                raise GateError(f"config init --kinds {kind} --tier {tier}: exit {rc}")
            # The fake GitHub is on an address of the host: the policy of the
            # operator opens it (an offer of the fake names no policy).
            with cfg.open("a") as f:
                f.write('\n[egress]\ndefault_policy = { schema = 1, mode = "open" }\n')
            serve = subprocess.Popen(["podrunner", "serve", "--config", str(cfg), "--state", str(d / "state")], cwd=d,
                                     env=env, stdout=(self.out / "logs" / f"job-{kind}-serve.out").open("w"),
                                     stderr=(self.out / "logs" / f"job-{kind}-serve.err").open("w"),
                                     start_new_session=True)
            procs.append(serve)
            job = json.dumps({"repository": "example-org/app", "tier": tier,
                              "steps": ["echo podrunner-gate-job-$((6*7))"]}).encode()
            req = urllib.request.Request(f"{ho_url}/_fake/jobs", data=job, method="POST",
                                         headers={"Content-Type": "application/json"})
            urllib.request.urlopen(req, timeout=30).read()
            reports, end = [], time.monotonic() + 900
            while time.monotonic() < end and serve.poll() is None and not reports:
                reports = json.loads(urllib.request.urlopen(f"{ho_url}/_fake/reports", timeout=30).read())
                time.sleep(5)
            jobs = json.loads(urllib.request.urlopen(f"{gh_url}/_fake/jobs", timeout=30).read())
            early = serve.poll()
        finally:
            for p in reversed(procs):
                if p.poll() is None:
                    os.killpg(p.pid, signal.SIGTERM)
                    try:
                        p.wait(timeout=120)
                    except subprocess.TimeoutExpired:
                        os.killpg(p.pid, signal.SIGKILL)
                        p.wait()
        report = reports[-1]["report"] if reports else {}
        conclusion = jobs[0].get("conclusion") if jobs else None
        # A trusted job on podvm runs only when serve proved the layer (T-566).
        boundary_ok = kind != "podvm" or report.get("boundary") == "confined"
        if report.get("exit_reason") != "success" or report.get("unit_kind") != kind or conclusion != "success" \
                or not boundary_ok:
            for log in (f"job-{kind}-serve", f"job-{kind}-githome", f"job-{kind}-github"):
                self.show(log, 120)
            ended = f"serve ended with exit {early}" if early is not None else "serve ran to the end of the wait"
            raise GateError(f"the job through serve on {kind}: {ended}; report {report or 'none'}, GitHub conclusion {conclusion}")
        return f"a runner job of githome through serve on {kind} ({tier}, {report.get('boundary')}): success, as GitHub says"

    def hold_args(self, kind: str) -> list:
        """The hold of a trusted podvm kind: podnet of the install."""
        return ["--podnet", self.data / "bin" / "podnet"] if kind == "podvm" else []

    def built_at(self) -> int:
        """`built_at_s` of the image of the lane in the manifest of the
        release, else now."""
        manifest = self.assets / "podrunner-manifest.json"
        if manifest.is_file():
            images = json.loads(manifest.read_text())["runner"]["images"].get(self.arch, {})
            digest = self.image.rpartition("@")[2]
            return next((i["built_at_s"] for i in images.values() if i and i.get("digest") == digest and i.get("built_at_s")),
                        int(time.time()))
        return int(time.time())

    def microvm_probe(self) -> str:
        """`podvm run --verbose` of the runner image on the machine tier, as
        the unit does it: the last lines name the cause of an early end
        (PodUtils/podvm#442)."""
        podvm = self.data / "bin" / "podvm"
        store = self.data / "images"
        # podvm asked for a run with no cached base archive, and one of
        # alpine (PodUtils/podvm#451).
        for base in (store / "machine").glob("base-*.cpio"):
            base.unlink(missing_ok=True)
        env = self.env(self.home, PODVM_STORE=str(store))
        out = []
        for image, log in ((self.image, "microvm-probe"), ("alpine:3.20", "microvm-probe-alpine")):
            argv = [podvm, "run", "--verbose", "--rm", "--podvm-tier=machine", "--podvm-mem", "8G", image, "true"]
            rc, _ = self.run(argv, env, self.work, log, timeout=600)
            err = (self.out / "logs" / f"{log}.err").read_text(errors="replace").splitlines()
            out.append(f"{image}: exit {rc}: " + " / ".join(line.strip() for line in err[-4:]))
        return " | ".join(out)

    def clone(self, repo: str, sha: str) -> pathlib.Path:
        d = self.work / "src" / f"{repo.replace('/', '_')}-{sha[:12]}"
        if not d.is_dir():
            for _ in range(3):
                shutil.rmtree(d, ignore_errors=True)
                if subprocess.run(["git", "clone", "-q", f"https://github.com/{repo}", str(d)]).returncode == 0:
                    break
            else:
                raise GateError(f"git clone https://github.com/{repo} failed")
            subprocess.run(["git", "-C", str(d), "checkout", "-q", "--detach", sha], check=True)
        return d

    # A run of hello.yml in the lab takes a few minutes: a hang ends at this
    # bound with its log, not at the limit of the job with none.
    LAB_TIMEOUT_S = 900

    def in_lab(self, profiles: str, argv, log: str) -> tuple[int, str]:
        """ARGV in the lab PROFILES of the installed podvm (`podvm lab run`,
        PodUtils/podvm#430): stdout and stderr together, exit 124 after
        LAB_TIMEOUT_S."""
        podvm = self.data / "bin" / "podvm"
        argv = [podvm, "lab", "run", profiles, "--", *argv]
        rc, out = self.run(argv, self.env(self.home), self.work, log, timeout=self.LAB_TIMEOUT_S)
        return rc, out + (self.out / "logs" / f"{log}.err").read_text(errors="replace")

    def lab_check(self, profiles: str) -> str:
        """The lab holds its class: podvm 0.2.0-beta.25 checks the profiles
        from inside the lab before CMD runs (`podvm lab check` alone
        measures the host outside the lab)."""
        podvm = self.data / "bin" / "podvm"
        rc, out = self.run([podvm, "lab", "run", profiles, "--", "true"], self.env(self.home), self.work, "lab-check")
        if rc:
            self.show("lab-check")
            raise GateError(f"podvm lab run {profiles} -- true: exit {rc}: the lab does not hold its class")
        return f"podvm lab run {profiles}: the lab holds its class"

    def doctor_names_the_bind(self, profiles: str) -> str:
        """T-213: on a host that refuses the bind, doctor --run fails and
        names the bind with its fix."""
        rc, out = self.in_lab(profiles, ["podrunner", "doctor", "--run", self.data / "hello.yml"], "lab-doctor")
        if rc == 0 or "FAIL loopback" not in out or "fix:" not in out:
            self.show("lab-doctor")
            raise GateError(f"doctor --run in the lab: exit {rc}, and no `FAIL loopback` with a fix")
        return "doctor --run says FAIL loopback, with its fix"

    def run_names_the_bind(self, profiles: str):
        """T-220 is open: a run on a host that refuses the bind cannot start
        its broker. It must fail and name the bind and a fix (T-213); when it
        passes, the lane must expect a pass."""
        rc, out = self.in_lab(profiles, ["podrunner", "run", self.data / "hello.yml"], "lab-run")
        if rc == 0:
            raise GateError("the run passes in the lab now (T-220): make the lane expect a pass")
        if "bind 127.0.0.1:0" not in out or "fix:" not in out:
            self.show("lab-run")
            raise GateError(f"the run in the lab failed (exit {rc}) with no bind and no fix named")
        return "xfail", "T-220: the run fails and names the bind and its fix"

    def run_on_the_vnet(self, profiles: str) -> str:
        """T-220: with --vnet, a run on a host that refuses the bind keeps
        its broker on a unix socket, and the runner of the unit reaches it
        through the vnet shim of podnet. It passes since T-561: a failure
        fails the lane."""
        podnet = self.data / "bin" / "podnet"
        rc, out = self.in_lab(profiles, ["podrunner", "run", self.data / "hello.yml", "--vnet", podnet], "lab-vnet")
        if rc == 0:
            return "a run with --vnet passes in the lab"
        self.show("lab-vnet")
        if rc == 124:
            raise GateError(f"the run with --vnet hung; stopped after {self.LAB_TIMEOUT_S} s (T-561)")
        tail = " / ".join(line for line in out.splitlines()[-3:] if line.strip())[:300]
        raise GateError(f"the run with --vnet failed (exit {rc}): {tail}")

    def wants(self, r: dict) -> bool:
        return r["executor"] in self.executors and (r["lanes"] == "*" or self.name in r["lanes"].split(","))

    def token_flags(self) -> list[str]:
        """The token of the gate for the jobs of the corpus, when the lane has
        one: the actions of real projects call the API of GitHub, and with no
        token the shared address of a hosted runner meets its rate limit
        (T-253). The token reads only the contents of podrunner."""
        return ["--github-token-env", "PODRUNNER_GATE_TOKEN"] if os.environ.get("PODRUNNER_GATE_TOKEN") else []

    def corpus_row(self, r: dict):
        r = {**r, "expect": r.get("expect_by_lane", {}).get(self.name, r["expect"])}
        src = self.clone(r["repo"], r["sha"])
        args = [r["workflow"], "--checkout", ".", "--job", r["job"]]
        for kv in [] if r["matrix"] == "-" else r["matrix"].split(";"):
            args += ["--matrix", kv]
        args += [] if r["flags"] == "-" else r["flags"].split()
        args += self.flags(r["executor"]) + self.token_flags()
        t0 = time.time()
        ok, detail = self.podrunner(args, src, f"{r['id']}-{r['executor']}", r["expect"] == "ok", ROW_TIMEOUT)
        times = self.gate_times(f"{r['id']}-{r['executor']}", t0)
        if r["expect"] == "ok":
            if not ok:
                raise GateError(f"github.com: success; {detail}{times}")
            return f"ok, as on github.com{times}"
        # A flaky row fails some runs for a known cause (its entry): either
        # result holds, and the table shows which one came.
        if r["expect"] == "flaky":
            return "pass, a flaky row" if ok else ("xfail", f"flaky: {r['note']} ({detail})")
        if ok:
            raise GateError("the job passes now (an xfail row): set expect to ok in scripts/corpus/user-gate.tsv")
        return "xfail", f"{r['note']} ({detail})"

    def gate_times(self, log: str, t0: float) -> str:
        """The `podrunner-gate-NAME=N` lines of the jobs of a row (the
        stand-in of rbg's gate, T-528): the seconds from the start of `run`
        to the first step, and each other time that a step prints."""
        found = {}
        for job in sorted((self.out / "logs" / log).glob("*/job.log")):
            for m in re.finditer(r"podrunner-gate-([a-z-]+)=(\d+)", job.read_text(errors="replace")):
                found.setdefault(m.group(1), int(m.group(2)))
        if not found:
            return ""
        if "first-step-at" in found:
            found["run-to-first-step-s"] = found.pop("first-step-at") - int(t0)
        return " (" + ", ".join(f"{k} {v}" for k, v in sorted(found.items())) + ")"

    def write(self):
        with (self.out / "results.tsv").open("w") as f:
            f.write("lane\tcheck\tresult\tdetail\n")
            for name, result, detail in self.results:
                f.write(f"{self.name}\t{name}\t{result}\t{detail}\n")


def lane(a) -> int:
    g = Lane(a)
    print(f"user-gate: lane {g.name}, as uid {os.getuid()}, podrunner {g.version}, {g.arch}, TMPDIR={os.environ.get('TMPDIR', '')}")
    rows = []
    if not a.no_corpus:
        rows, errors = read_table(TABLE)
        if errors:
            raise SystemExit("\n".join(errors))
    later = [f"hello:{e}" for e in g.executors] + (["errors"] if "runner-dir" in g.executors else []) + (["rung:podvm", "confine:podvm", "pids:podvm", "pty:podvm", "hold:podvm"] if "podvm" in g.executors else [])
    later += [f"serve:{c}:{k}" for k in g.serve_kinds for c in ("check", "run", "job")]
    later += [f"corpus:{r['id']}:{r['executor']}" for r in rows if g.wants(r)]
    try:
        if not g.check("serve", g.serve):
            for name in later:
                g.skip(name, "no local server")
        else:
            g.check("docs:quick-start", g.quick_start)
            if g.check("install:bundle", g.install_bundle):
                g.check("install:offline-doc", g.install_offline_doc)
                # With no larger image, the rows run on the base image.
                g.check("install:full", g.install_full) or g.check("install:base-image", g.run_toml_image)
                # A host class that podvm makes around podrunner (T-244).
                if a.podvm_lab:
                    if g.check("lab:check", functools.partial(g.lab_check, a.podvm_lab)):
                        g.check("lab:doctor", functools.partial(g.doctor_names_the_bind, a.podvm_lab))
                        g.check("lab:run", functools.partial(g.run_names_the_bind, a.podvm_lab))
                        g.check("lab:vnet", functools.partial(g.run_on_the_vnet, a.podvm_lab))
                for e in g.executors:
                    g.check(f"hello:{e}", functools.partial(g.hello, e))
                if "runner-dir" in g.executors:
                    g.check("errors", g.error_paths)
                if "podvm" in g.executors:
                    g.check("rung:podvm", g.rung)
                    g.check("confine:podvm", functools.partial(g.confine, "podvm"))
                    g.check("pids:podvm", functools.partial(g.pids, "podvm"))
                    g.check("pty:podvm", g.pty)
                    g.check("hold:podvm", g.hold)
                for k in g.serve_kinds:
                    if g.check(f"serve:check:{k}", functools.partial(g.serve_check, k)):
                        g.check(f"serve:run:{k}", functools.partial(g.serve_run, k))
                        g.check(f"serve:job:{k}", functools.partial(g.serve_job, k))
                    else:
                        g.skip(f"serve:run:{k}", "the check of the configuration failed")
                        g.skip(f"serve:job:{k}", "the check of the configuration failed")
                for r in rows:
                    if g.wants(r):
                        g.check(f"corpus:{r['id']}:{r['executor']}", functools.partial(g.corpus_row, r))
            else:
                for name in later:
                    g.skip(name, "the install failed")
    finally:
        g.write()
    failed = [n for n, result, _ in g.results if result == "fail"]
    print(f"user-gate: lane {g.name}: {len(g.results) - len(failed)} of {len(g.results)} checks passed")
    return 1 if failed else 0


def summary(a) -> int:
    results: dict[str, dict[str, tuple[str, str]]] = {}
    checks: list[str] = []
    for path in sorted(pathlib.Path(a.dir).rglob("results.tsv")):
        for line in path.read_text().splitlines()[1:]:
            name, check, result, detail = (line.split("\t") + ["", "", "", ""])[:4]
            results.setdefault(name, {})[check] = (result, detail)
            if check not in checks:
                checks.append(check)
    lanes = a.lanes.split(",") if a.lanes else sorted(results)
    mark = {"pass": "pass", "xfail": "xfail", "skip": "skip", "fail": "**FAIL**"}
    print("## The user-path gate\n")
    print("| check | " + " | ".join(lanes) + " |")
    print("| --- |" + " --- |" * len(lanes))
    bad = 0
    for c in checks:
        cells = []
        for ln in lanes:
            r = results.get(ln, {}).get(c)
            cells.append(mark.get(r[0], r[0]) if r else "-")
            bad += bool(r and r[0] == "fail")
        print(f"| `{c}` | " + " | ".join(cells) + " |")
    missing = [ln for ln in lanes if ln not in results]
    for ln in missing:
        print(f"\nThe lane {ln} gave no results.")
    notes = [(ln, c, d) for ln in lanes for c, (r, d) in results.get(ln, {}).items() if r in ("fail", "xfail")]
    if notes:
        print("\n| lane | check | why |\n| --- | --- | --- |")
        for ln, c, d in notes:
            print(f"| {ln} | `{c}` | {d.replace('|', '/')} |")
    return 1 if bad or missing else 0


def check(_a) -> int:
    rows, errors = read_table(TABLE)
    try:
        qs = doc_commands(INSTALL_DOC, QUICK_START, [])
        if RELEASE_URL not in qs:
            errors.append(f"{INSTALL_DOC.name}, {QUICK_START}: no {RELEASE_URL}: the lane cannot serve it")
        doc_commands(INSTALL_DOC, OFFLINE, [])
    except GateError as e:
        errors.append(str(e))
    for e in errors:
        print(e)
    if errors:
        return 1
    print(f"user-gate: {len(rows)} rows of the corpus; the sections of {INSTALL_DOC.name} have their commands")
    return 0


def main() -> int:
    p = argparse.ArgumentParser(description="The user-path gate (T-243).")
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("check")
    d = sub.add_parser("doc-commands")
    d.add_argument("file")
    d.add_argument("section")
    d.add_argument("--set", action="append", default=[], metavar="OLD=NEW")
    ln = sub.add_parser("lane")
    ln.add_argument("--lane", required=True)
    ln.add_argument("--assets", required=True)
    ln.add_argument("--out", required=True)
    ln.add_argument("--work")
    ln.add_argument("--executors", default="podvm,runner-dir")
    ln.add_argument("--no-corpus", action="store_true")
    ln.add_argument("--podvm-lab", default="", metavar="PROFILES")
    ln.add_argument("--serve-kinds", default="", metavar="KINDS")
    ln.add_argument("--want-rung", default="", metavar="RUNG")
    ln.add_argument("--schema-python", default="", metavar="PATH")
    s = sub.add_parser("summary")
    s.add_argument("--lanes", default="")
    s.add_argument("dir")
    a = p.parse_args()
    if a.cmd == "doc-commands":
        sets = [tuple(x.split("=", 1)) for x in a.set]
        try:
            sys.stdout.write(doc_commands(pathlib.Path(a.file), a.section, sets))
        except GateError as e:
            print(e, file=sys.stderr)
            return 1
        return 0
    return {"check": check, "lane": lane, "summary": summary}[a.cmd](a)


if __name__ == "__main__":
    sys.exit(main())
