#!/usr/bin/env python3
"""Poll /proc for the descendants of ROOTPID and report, per process, what its argv and environ held.

usage: sampler.py ROOTPID SECONDS BLOBFILE RSA_D_FILE
Never prints the blob or key. Reports booleans per pid:
  argv_blob   argv contains the first 60 chars of the jitconfig blob
  env_name    environ contains the name ACTIONS_RUNNER_INPUT_JITCONFIG
  env_blob    environ contains the first 60 chars of the blob
  sentinel    argv or environ contains SENTINEL-CLIENT-ID / SENTINEL-AGENT-NAME / the RSA d value prefix
Reads /proc/PID/{cmdline,environ,comm}. environ is the initial block of the process (what exec gave it), which is also what
`ps e` and any same-uid reader sees. Poll interval 20 ms.
"""
import os, sys, time

root, secs = int(sys.argv[1]), float(sys.argv[2])
blob = open(sys.argv[3]).read().strip()[:60].encode()
rsad = open(sys.argv[4]).read().strip()[:40].encode()
sent = [b"SENTINEL-CLIENT-ID", b"SENTINEL-AGENT-NAME", rsad]
seen = {}


def ppid_map():
    m = {}
    for d in os.listdir("/proc"):
        if d.isdigit():
            try:
                with open("/proc/%s/stat" % d, "rb") as f:
                    s = f.read()
                m[int(d)] = int(s[s.rindex(b")") + 2:].split()[1])
            except OSError:
                pass
    return m


def read(p):
    try:
        with open(p, "rb") as f:
            return f.read()
    except OSError:
        return b""


end = time.time() + secs
while time.time() < end:
    pp = ppid_map()
    tree = {root}
    changed = True
    while changed:
        changed = False
        for pid, par in pp.items():
            if par in tree and pid not in tree:
                tree.add(pid)
                changed = True
    for pid in tree:
        if pid not in pp:
            continue
        a = read("/proc/%d/cmdline" % pid)
        e = read("/proc/%d/environ" % pid)
        if not a and not e:
            continue
        r = seen.setdefault(pid, {"comm": read("/proc/%d/comm" % pid).decode().strip(), "argv0": a.split(b"\0")[0].decode(errors="replace").split("/")[-1],
                                  "argv_blob": False, "env_name": False, "env_blob": False, "sentinel": False, "n": 0})
        r["n"] += 1
        r["argv_blob"] |= blob in a
        r["env_name"] |= b"ACTIONS_RUNNER_INPUT_JITCONFIG" in e
        r["env_blob"] |= blob in e
        r["sentinel"] |= any(s in a or s in e for s in sent)
    time.sleep(0.02)
for pid, r in sorted(seen.items()):
    print("pid_seen comm=%-16s argv0=%-18s samples=%-4d argv_blob=%s env_name=%s env_blob=%s sentinel=%s" % (
        r["comm"], r["argv0"], r["n"], r["argv_blob"], r["env_name"], r["env_blob"], r["sentinel"]))
