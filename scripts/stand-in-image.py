# A stand-in payload image with glibc, for the podvm driver tests on a host
# with no root (the userland rung of podvm loads a payload with an embedded
# glibc and needs libdl and libgcc_s in /lib or /lib64; busybox has none).
# It copies host binaries and their libraries into one layer and writes an
# OCI layout tarball; `podvm load -i FILE` reads it. The digest of the
# manifest goes to stdout.
#
#   python3 -I scripts/stand-in-image.py OUT.tar
import gzip, hashlib, io, json, os, subprocess, sys, tarfile
out = sys.argv[1]
bins = ["/bin/sh", "/bin/dash", "/bin/sleep", "/usr/bin/setsid", "/bin/cat", "/bin/ls", "/bin/grep", "/usr/bin/env", "/usr/bin/id", "/bin/echo", "/usr/bin/test", "/usr/bin/tr", "/bin/mkdir", "/bin/rm"]
libs = set(["/lib/x86_64-linux-gnu/libgcc_s.so.1", "/lib/x86_64-linux-gnu/libdl.so.2", "/lib/x86_64-linux-gnu/libpthread.so.0", "/lib/x86_64-linux-gnu/librt.so.1", "/lib/x86_64-linux-gnu/libm.so.6"])
for b in bins:
    r = subprocess.run(["ldd", os.path.realpath(b)], capture_output=True, text=True)
    for line in r.stdout.splitlines():
        parts = line.split()
        for p in parts:
            if p.startswith("/") and os.path.exists(p):
                libs.add(p)
layer = io.BytesIO()
with tarfile.open(fileobj=layer, mode="w") as t:
    def d(name):
        ti = tarfile.TarInfo(name); ti.type = tarfile.DIRTYPE; ti.mode = 0o755; t.addfile(ti)
    for n in ["bin", "usr", "usr/bin", "lib", "lib64", "tmp", "home"]:
        d(n)
    def f(src, dst, mode=0o755):
        data = open(os.path.realpath(src), "rb").read()
        ti = tarfile.TarInfo(dst); ti.size = len(data); ti.mode = mode; t.addfile(ti, io.BytesIO(data))
    for b in bins:
        f(b, "bin/" + os.path.basename(b))
    for l in sorted(libs):
        name = os.path.basename(l)
        f(l, "lib/" + name)
        f(l, "lib64/" + name)
raw = layer.getvalue()
gz = gzip.compress(raw, mtime=0)
diff = "sha256:" + hashlib.sha256(raw).hexdigest()
blobs = {}
def blob(data):
    h = hashlib.sha256(data).hexdigest(); blobs[h] = data; return "sha256:" + h, len(data)
ldig, lsize = blob(gz)
config = json.dumps({"architecture": "amd64", "os": "linux", "config": {"Env": ["PATH=/bin:/usr/bin"]}, "rootfs": {"type": "layers", "diff_ids": [diff]}}).encode()
cdig, csize = blob(config)
manifest = json.dumps({"schemaVersion": 2, "mediaType": "application/vnd.oci.image.manifest.v1+json",
    "config": {"mediaType": "application/vnd.oci.image.config.v1+json", "digest": cdig, "size": csize},
    "layers": [{"mediaType": "application/vnd.oci.image.layer.v1.tar+gzip", "digest": ldig, "size": lsize}]}).encode()
mdig, msize = blob(manifest)
index = json.dumps({"schemaVersion": 2, "manifests": [{"mediaType": "application/vnd.oci.image.manifest.v1+json", "digest": mdig, "size": msize,
    "annotations": {"org.opencontainers.image.ref.name": "glibc-mini:1"}}]}).encode()
with tarfile.open(out, "w") as t:
    def add(name, data):
        ti = tarfile.TarInfo(name); ti.size = len(data); ti.mode = 0o644; t.addfile(ti, io.BytesIO(data))
    add("oci-layout", b'{"imageLayoutVersion":"1.0.0"}')
    add("index.json", index)
    for h, data in blobs.items():
        add("blobs/sha256/" + h, data)
print(mdig)
