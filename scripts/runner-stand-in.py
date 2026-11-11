# A stand-in of the runner image (images/runner/) for a test on a host
# with no docker: an unpacked official runner in /home/runner, the shell
# tools that run.sh calls, and the host libraries that they and .NET need
# (ICU, OpenSSL and libstdc++ are loaded at run time, so they are named
# here). The credential files of the runner are links to the runner root
# of the podvm driver, as in images/runner/Dockerfile. The output is a
# plain rootfs tar for `podvm import TAR NAME`.
#
#   python3 -I scripts/runner-stand-in.py RUNNER_DIR OUT.tar
import glob, io, os, subprocess, sys, tarfile

runner, out = sys.argv[1], sys.argv[2]
tools = ["sh", "bash", "dash", "cat", "cp", "mkdir", "rm", "sleep", "ls", "grep", "sed", "readlink", "dirname",
         "basename", "uname", "env", "id", "tr", "head", "tail", "printf", "echo", "test", "true", "false", "mv",
         "chmod", "ln", "date", "cut", "touch", "wc", "find", "xargs", "sort", "awk", "ps", "kill", "tee", "realpath"]
bins = [b for b in (subprocess.run(["sh", "-c", f"command -v {t}"], capture_output=True, text=True).stdout.strip()
                    for t in tools) if b.startswith("/")]
libdir = "/lib/x86_64-linux-gnu"
extra = []
for pat in ["libicu*.so*", "libssl.so*", "libcrypto.so*", "libstdc++.so*", "libgcc_s.so*", "libz.so*",
            "libdl.so*", "libpthread.so*", "librt.so*", "libm.so*", "liblttng*.so*", "libgssapi_krb5.so*",
            "libkrb5*.so*", "libk5crypto.so*", "libcom_err.so*", "libkeyutils.so*", "libresolv.so*"]:
    extra += glob.glob(os.path.join(libdir, pat))
libs = set(extra)
objects = bins + extra + glob.glob(os.path.join(runner, "bin", "*.so")) + [os.path.join(runner, "bin", "Runner.Listener")]
for o in objects:
    r = subprocess.run(["ldd", os.path.realpath(o)], capture_output=True, text=True)
    for part in r.stdout.split():
        if part.startswith("/") and os.path.exists(part):
            libs.add(part)

with tarfile.open(out, "w") as t:
    def d(name, mode=0o755):
        ti = tarfile.TarInfo(name); ti.type = tarfile.DIRTYPE; ti.mode = mode; t.addfile(ti)
    def f(src, dst, mode=None):
        data = open(os.path.realpath(src), "rb").read()
        ti = tarfile.TarInfo(dst); ti.size = len(data)
        ti.mode = mode if mode is not None else (os.stat(os.path.realpath(src)).st_mode & 0o777)
        t.addfile(ti, io.BytesIO(data))
    def link(name, target):
        ti = tarfile.TarInfo(name); ti.type = tarfile.SYMTYPE; ti.linkname = target; t.addfile(ti)
    for n in ["bin", "usr", "usr/bin", "lib", "lib64", "lib/x86_64-linux-gnu", "tmp", "home", "etc", "podrunner"]:
        d(n, 0o1777 if n == "tmp" else 0o755)
    for b in bins:
        f(b, "bin/" + os.path.basename(b), 0o755)
        link("usr/bin/" + os.path.basename(b), "/bin/" + os.path.basename(b))
    for l in sorted(libs):
        f(l, "lib/x86_64-linux-gnu/" + os.path.basename(l), 0o755)
        if os.path.basename(l).startswith("ld-linux"):
            f(l, "lib64/" + os.path.basename(l), 0o755)
    ti = tarfile.TarInfo("etc/passwd"); data = b"root:x:0:0::/root:/bin/sh\nrunner:x:1001:1001::/home/runner:/bin/sh\n"
    ti.size = len(data); ti.mode = 0o644; t.addfile(ti, io.BytesIO(data))
    skip = {"_work", "_diag", ".runner", ".credentials", ".credentials_rsaparams"}
    for root, dirs, files in os.walk(runner):
        dirs[:] = [x for x in dirs if not (root == runner and x in skip)]
        rel = os.path.relpath(root, runner)
        base = "home/runner" if rel == "." else "home/runner/" + rel
        d(base)
        for name in files:
            if root == runner and name in skip:
                continue
            p = os.path.join(root, name)
            if os.path.islink(p):
                link(base + "/" + name, os.readlink(p))
            else:
                f(p, base + "/" + name)
    # Relative links: on the supervise rung of podvm the kernel follows an
    # absolute link on the host, out of the rootfs.
    for name in [".runner", ".credentials", ".credentials_rsaparams"]:
        link("home/runner/" + name, "../../podrunner/.runner/" + name)
print(out)
