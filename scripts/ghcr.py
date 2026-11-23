#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The images of a release on ghcr.io (T-553; the form of COORD-142 of podcrates).

    python3 -I scripts/ghcr.py binary --name NAME --tag TAG --revision SHA --out DIR ARCH=FILE...
    python3 -I scripts/ghcr.py archives --tag TAG --revision SHA --out DIR ARCH=ARCHIVE...
    python3 -I scripts/ghcr.py check-binary --name NAME DIR ARCH=FILE...
    python3 -I scripts/ghcr.py podvm-archive --arch ARCH --ref REF DIR OUT.tar
    REGISTRY_TOKEN=... python3 -I scripts/ghcr.py push --registry URL --repo NAME DIR
    REGISTRY_TOKEN=... python3 -I scripts/ghcr.py verify --registry URL --repo NAME DIR
    python3 -I scripts/ghcr.py --self-test

ARCH is x86_64 or aarch64. Each command that writes DIR writes an OCI
layout: `index.json` names one multi-arch index by the ref name TAG, and
each blob is in `blobs/sha256/`. It prints the digest of the index.

`binary` makes one image for each FILE, with no base (`FROM scratch`):
FILE at `/usr/local/bin/NAME` (mode 0755, owner 0), an empty `/tmp`, `ENTRYPOINT
["/usr/local/bin/NAME"]`, and the labels `org.opencontainers.image.source`,
`version` (TAG), `revision` (SHA), `licenses` and `title`. The layer is a
gzip tar with no time stamp, so the same files give the same digests.
`archives` takes the runner image archives (`runner-ARCH.tar`, the OCI
layout that `podvm load -i` takes, one manifest each) with their bytes as
they are: the digest of each platform is the digest that the installer
pins. `check-binary` reads each platform image of DIR back and compares
its `/usr/local/bin/NAME` with FILE by SHA-256. `podvm-archive` writes
the image of one ARCH of DIR as the archive that `podvm load -i` takes,
with the ref name REF, and prints its manifest digest.

`push` sends each blob, each manifest by its digest and the index at TAG
(`scripts/hosted/push-blob.py`). `verify` reads TAG back: the registry
must give the index of DIR, each manifest that its index names with the
same digest, and each blob. It prints one line for the index and one for
each platform: `PLATFORM DIGEST` (the platform of the index is `each`).
The token of the registry comes from the environment, never from argv.
"""

import gzip
import hashlib
import importlib.util
import io
import json
import os
import pathlib
import re
import sys
import tarfile


def _load(name: str, file: str, where: str = "hosted"):
    spec = importlib.util.spec_from_file_location(name, pathlib.Path(__file__).resolve().parent / where / file)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


oci = _load("podrunner_oci", "oci.py")
pb = _load("podrunner_push_blob", "push-blob.py")

GZIP_LAYER = "application/vnd.oci.image.layer.v1.tar+gzip"
REF = "org.opencontainers.image.ref.name"
TAG = re.compile(r"^[A-Za-z0-9_][A-Za-z0-9._-]{0,127}$")
NAME = re.compile(r"^[a-z0-9][a-z0-9._-]*$")
REVISION = re.compile(r"^[0-9a-f]{40}$")
DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")


def sha(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def put(out: pathlib.Path, data: bytes) -> dict:
    d = sha(data)
    p = out / "blobs" / "sha256" / d[7:]
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_bytes(data)
    return {"digest": d, "size": len(data)}


def path_of(out: pathlib.Path, digest: str) -> pathlib.Path:
    if not DIGEST.match(digest):
        raise ValueError(f"{digest!r} is not sha256:HEX")
    return out / "blobs" / "sha256" / digest[7:]


def labels(tag: str, revision: str, title: str, licenses: str | None) -> dict:
    if not TAG.match(tag):
        raise ValueError(f"{tag!r} is not a tag of a registry")
    if not REVISION.match(revision):
        raise ValueError(f"{revision!r} is not the SHA of a commit")
    lab = {"org.opencontainers.image.source": oci.SOURCE, "org.opencontainers.image.version": tag,
           "org.opencontainers.image.revision": revision, "org.opencontainers.image.title": title}
    if licenses:
        lab["org.opencontainers.image.licenses"] = licenses
    return lab


def finish(out: pathlib.Path, entries: list[dict], tag: str, notes: dict) -> str:
    """The multi-arch index of ENTRIES as a blob, and index.json that names it by TAG."""
    if not entries:
        raise ValueError("no image")
    arches = [e["platform"]["architecture"] for e in entries]
    if len(set(arches)) != len(arches):
        raise ValueError(f"two images of one arch: {arches}")
    entries = sorted(entries, key=lambda e: e["platform"]["architecture"])
    d = put(out, oci.canon({"schemaVersion": 2, "mediaType": oci.INDEX, "manifests": entries, "annotations": notes}))
    top = {"schemaVersion": 2, "mediaType": oci.INDEX,
           "manifests": [{"mediaType": oci.INDEX, **d, "annotations": {REF: tag}}]}
    (out / "index.json").write_bytes(oci.canon(top))
    (out / "oci-layout").write_text('{"imageLayoutVersion":"1.0.0"}\n')
    return d["digest"]


def pairs(args: list[str]) -> list[tuple[str, pathlib.Path]]:
    out = []
    for a in args:
        arch, eq, f = a.partition("=")
        if not eq or arch not in oci.ARCH:
            raise ValueError(f"{a!r} is not ARCH=FILE with ARCH in {sorted(oci.ARCH)}")
        out.append((arch, pathlib.Path(f)))
    if len({a for a, _ in out}) != len(out):
        raise ValueError(f"an arch twice: {args}")
    return out


def layer(name: str, data: bytes) -> bytes:
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w", format=tarfile.USTAR_FORMAT) as t:
        # An empty /tmp: a run of the image (podvm, docker) has a place for
        # its temporary files.
        for d, mode in (("tmp", 0o1777), ("usr", 0o755), ("usr/local", 0o755), ("usr/local/bin", 0o755)):
            ti = tarfile.TarInfo(d)
            ti.type, ti.mode, ti.mtime = tarfile.DIRTYPE, mode, 0
            t.addfile(ti)
        ti = tarfile.TarInfo(f"usr/local/bin/{name}")
        ti.size, ti.mode, ti.mtime = len(data), 0o755, 0
        t.addfile(ti, io.BytesIO(data))
    return buf.getvalue()


def binary(name: str, tag: str, revision: str, files: list[tuple[str, pathlib.Path]], out: pathlib.Path) -> str:
    if not NAME.match(name):
        raise ValueError(f"{name!r} is not a name of an image")
    lab = labels(tag, revision, name, "0BSD")
    entries = []
    for arch, f in files:
        data = f.read_bytes()
        if data[:4] != b"\x7fELF":
            raise ValueError(f"{f}: not an ELF file")
        tar = layer(name, data)
        gz = gzip.compress(tar, compresslevel=9, mtime=0)
        cfg = {"architecture": oci.ARCH[arch], "os": "linux",
               "config": {"Entrypoint": [f"/usr/local/bin/{name}"], "Labels": lab},
               "rootfs": {"type": "layers", "diff_ids": [sha(tar)]},
               "history": [{"created_by": f"podrunner release: {f.name} at /usr/local/bin/{name}"}]}
        man = {"schemaVersion": 2, "mediaType": oci.MANIFEST,
               "config": {"mediaType": oci.CONFIG, **put(out, oci.canon(cfg))},
               "layers": [{"mediaType": GZIP_LAYER, **put(out, gz)}], "annotations": lab}
        data = oci.canon(man)
        put(out, data)
        entries.append(oci.entry(data, oci.ARCH[arch]))
    return finish(out, entries, tag, lab)


def member_name(n: str) -> str:
    return n[2:] if n.startswith("./") else n


def copy_blob(t: tarfile.TarFile, members: dict, desc: dict, out: pathlib.Path, where: str,
              keep: bool = False) -> bytes:
    """Copy the blob of DESC from the archive T to OUT and check its digest
    and size; with KEEP, give its bytes (a manifest or a config: small)."""
    digest = desc["digest"]
    dest = path_of(out, digest)
    m = members.get(f"blobs/sha256/{dest.name}")
    if m is None:
        raise ValueError(f"{where}: no blob {digest}")
    if keep and m.size > 1 << 22:
        raise ValueError(f"{where}: the blob {digest} has {m.size} bytes: too large for a manifest or a config")
    dest.parent.mkdir(parents=True, exist_ok=True)
    h, n, kept = hashlib.sha256(), 0, io.BytesIO()
    with t.extractfile(m) as src, open(dest, "wb") as f:
        while b := src.read(1 << 20):
            h.update(b)
            n += len(b)
            f.write(b)
            if keep:
                kept.write(b)
    if "sha256:" + h.hexdigest() != digest or ("size" in desc and n != desc["size"]):
        dest.unlink()
        raise ValueError(f"{where}: the blob {digest} has other bytes")
    return kept.getvalue()


def archives(tag: str, revision: str, files: list[tuple[str, pathlib.Path]], out: pathlib.Path) -> str:
    lab = labels(tag, revision, "podrunner runner image", None)
    entries = []
    for arch, f in files:
        with tarfile.open(f) as t:
            members = {member_name(m.name): m for m in t.getmembers() if m.isfile()}
            if "index.json" not in members:
                raise ValueError(f"{f}: no index.json: not an OCI layout")
            ix = json.load(t.extractfile(members["index.json"]))
            if len(ix.get("manifests", [])) != 1:
                raise ValueError(f"{f}: {len(ix.get('manifests', []))} manifests in index.json, not one")
            desc = ix["manifests"][0]
            data = copy_blob(t, members, desc, out, str(f), keep=True)
            e = oci.entry(data, oci.ARCH[arch])
            m = json.loads(data)
            cfg = json.loads(copy_blob(t, members, m["config"], out, str(f), keep=True))
            if (cfg.get("architecture"), cfg.get("os")) != (oci.ARCH[arch], "linux"):
                have = f"{cfg.get('os')}/{cfg.get('architecture')}"
                raise ValueError(f"{f}: the image is {have}, not linux/{oci.ARCH[arch]}")
            for lay in m["layers"]:
                copy_blob(t, members, lay, out, str(f))
            entries.append(e)
    return finish(out, entries, tag, lab)


def read_layout(d: pathlib.Path) -> tuple[str, str, bytes]:
    """The tag, the digest and the bytes of the index of the layout D."""
    top = json.loads((d / "index.json").read_bytes())
    if len(top.get("manifests", [])) != 1 or top["manifests"][0].get("mediaType") != oci.INDEX:
        raise ValueError(f"{d}: index.json must name one index")
    desc = top["manifests"][0]
    tag = desc.get("annotations", {}).get(REF, "")
    if not TAG.match(tag):
        raise ValueError(f"{d}: the ref name {tag!r} is not a tag")
    data = path_of(d, desc["digest"]).read_bytes()
    if sha(data) != desc["digest"]:
        raise ValueError(f"{d}: the index has other bytes than {desc['digest']}")
    return tag, desc["digest"], data


def check_binary(name: str, d: pathlib.Path, files: list[tuple[str, pathlib.Path]]) -> list[str]:
    _, _, data = read_layout(d)
    by_arch = {e["platform"]["architecture"]: e for e in json.loads(data)["manifests"]}
    want = {oci.ARCH[a]: f for a, f in files}
    if set(by_arch) != set(want):
        raise ValueError(f"{d}: the images are {sorted(by_arch)}, not {sorted(want)}")
    rows = []
    for arch, f in sorted(want.items()):
        m = json.loads(path_of(d, by_arch[arch]["digest"]).read_bytes())
        cfg = json.loads(path_of(d, m["config"]["digest"]).read_bytes())
        if cfg["config"].get("Entrypoint") != [f"/usr/local/bin/{name}"] or cfg["architecture"] != arch:
            raise ValueError(f"{d} {arch}: the config is {cfg['architecture']} {cfg['config'].get('Entrypoint')}")
        if len(m["layers"]) != 1:
            raise ValueError(f"{d} {arch}: {len(m['layers'])} layers, not one")
        with tarfile.open(fileobj=io.BytesIO(gzip.decompress(path_of(d, m["layers"][0]["digest"]).read_bytes()))) as t:
            mem = t.getmember(f"usr/local/bin/{name}")
            got = hashlib.sha256(t.extractfile(mem).read()).hexdigest()
        exp = hashlib.sha256(f.read_bytes()).hexdigest()
        if got != exp or mem.mode != 0o755:
            raise ValueError(f"{d} linux/{arch}: /usr/local/bin/{name} is not {f.name} ({got}, mode {mem.mode:o})")
        rows.append(f"linux/{arch} {by_arch[arch]['digest']} {f.name} {exp}")
    return rows


def push(reg, repo: str, d: pathlib.Path) -> str:
    tag, digest, data = read_layout(d)
    for e in json.loads(data)["manifests"]:
        m = json.loads(path_of(d, e["digest"]).read_bytes())
        for b in [m["config"], *m["layers"]]:
            got = pb.push(reg, repo, str(path_of(d, b["digest"])))
            if got != b["digest"]:
                raise RuntimeError(f"the blob {b['digest']} went up as {got}")
        pb.push_manifest(reg, repo, e["digest"], str(path_of(d, e["digest"])), e["mediaType"])
    got = pb.push_manifest(reg, repo, tag, str(path_of(d, digest)), oci.INDEX)
    if got != digest:
        raise RuntimeError(f"the index of {tag} went up as {got}, not {digest}")
    return digest


def get(reg, path: str, accept: str) -> bytes:
    r = reg.request("GET", path, headers={"Accept": accept})
    if r.status != 200:
        raise RuntimeError(f"GET {path}: HTTP {r.status}")
    return r.read_body


def verify(reg, repo: str, d: pathlib.Path) -> list[str]:
    tag, digest, _ = read_layout(d)
    data = get(reg, f"/v2/{repo}/manifests/{tag}", oci.INDEX)
    if sha(data) != digest:
        raise RuntimeError(f"{repo}:{tag} is {sha(data)}, not {digest}")
    rows = [f"each {digest}"]
    # Each platform from the index that the registry gives.
    for e in json.loads(data)["manifests"]:
        m = get(reg, f"/v2/{repo}/manifests/{e['digest']}", e["mediaType"])
        if sha(m) != e["digest"]:
            raise RuntimeError(f"{repo}@{e['digest']}: the registry gives {sha(m)}")
        for b in [json.loads(m)["config"], *json.loads(m)["layers"]]:
            s = reg.request("HEAD", f"/v2/{repo}/blobs/{b['digest']}").status
            if s not in (200, 307):
                raise RuntimeError(f"{repo}: the blob {b['digest']}: HTTP {s}")
        rows.append(f"linux/{e['platform']['architecture']} {e['digest']}")
    return rows


def podvm_archive(d: pathlib.Path, arch: str, ref: str, out: pathlib.Path) -> str:
    """The archive of the ARCH image of D that `podvm load -i` takes: its
    manifest with the ref name REF, and only the blobs that it names."""
    _, _, data = read_layout(d)
    es = [e for e in json.loads(data)["manifests"] if e["platform"]["architecture"] == oci.ARCH.get(arch)]
    if len(es) != 1:
        raise ValueError(f"{d}: no image of {arch}")
    e = es[0]
    m = json.loads(path_of(d, e["digest"]).read_bytes())
    top = oci.canon({"schemaVersion": 2, "mediaType": oci.INDEX, "manifests": [{**e, "annotations": {REF: ref}}]})
    with tarfile.open(out, "w") as t:
        for name, b in (("oci-layout", (d / "oci-layout").read_bytes()), ("index.json", top)):
            ti = tarfile.TarInfo(name)
            ti.size, ti.mode = len(b), 0o644
            t.addfile(ti, io.BytesIO(b))
        for digest in (e["digest"], m["config"]["digest"], *(lay["digest"] for lay in m["layers"])):
            t.add(path_of(d, digest), f"blobs/sha256/{digest[7:]}")
    return e["digest"]


def main(a: list[str]) -> int:
    if a == ["--self-test"]:
        return _load("podrunner_ghcr_test", "ghcr_test.py", ".").self_test(sys.modules[__name__])
    if a[:1] == ["binary"] and len(a) >= 10 and a[1:8:2] == ["--name", "--tag", "--revision", "--out"]:
        print(binary(a[2], a[4], a[6], pairs(a[9:]), pathlib.Path(a[8])))
        return 0
    if a[:1] == ["archives"] and len(a) >= 8 and a[1:6:2] == ["--tag", "--revision", "--out"]:
        print(archives(a[2], a[4], pairs(a[7:]), pathlib.Path(a[6])))
        return 0
    if a[:1] == ["podvm-archive"] and len(a) == 7 and a[1:4:2] == ["--arch", "--ref"]:
        print(podvm_archive(pathlib.Path(a[5]), a[2], a[4], pathlib.Path(a[6])))
        return 0
    if a[:2] == ["check-binary", "--name"] and len(a) >= 5:
        print("\n".join(check_binary(a[2], pathlib.Path(a[3]), pairs(a[4:]))))
        return 0
    if a[:1] in (["push"], ["verify"]) and len(a) == 6 and a[1:4:2] == ["--registry", "--repo"]:
        if not re.match(r"^[a-z0-9][a-z0-9._-]*(/[a-z0-9][a-z0-9._-]*)+$", a[4]):
            raise ValueError(f"{a[4]!r} is not a repository of a registry")
        reg = pb.Registry(a[2], os.environ.get("REGISTRY_TOKEN"))
        if a[0] == "push":
            print(push(reg, a[4], pathlib.Path(a[5])))
        else:
            print("\n".join(verify(reg, a[4], pathlib.Path(a[5]))))
        return 0
    print("\n".join(__doc__.strip().splitlines()[2:9]), file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
