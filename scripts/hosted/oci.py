#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The OCI config, manifest and index of the hosted snapshot (T-555).

    python3 -I scripts/hosted/oci.py image --layers LAYERS.TSV --record SNAPSHOT.JSON --out DIR
    python3 -I scripts/hosted/oci.py index --out DIR MANIFEST.JSON...
    python3 -I scripts/hosted/oci.py --self-test

`image` reads the layer table of a capture (`capture.sh`: name, diff id,
digest, size, check) and its record, and writes DIR/config.json and
DIR/manifest.json, each with the digest of its exact bytes in a file
beside it (`.digest`). The layers are `tar+zstd`; `boot` is the last. The
annotations name the hosted image (ImageOS, ImageVersion, the kernel) and
the source of podrunner. `index` writes one multi-arch index of the
manifests (each with its platform). The bytes are canonical (sorted keys,
no spaces), so the same capture gives the same digests.
"""

import hashlib
import json
import pathlib
import sys

LAYER = "application/vnd.oci.image.layer.v1.tar+zstd"
CONFIG = "application/vnd.oci.image.config.v1+json"
MANIFEST = "application/vnd.oci.image.manifest.v1+json"
INDEX = "application/vnd.oci.image.index.v1+json"
SOURCE = "https://github.com/PodUtils/podrunner"
ARCH = {"x86_64": "amd64", "aarch64": "arm64"}


def canon(obj) -> bytes:
    return json.dumps(obj, sort_keys=True, separators=(",", ":")).encode()


def put(out: pathlib.Path, name: str, data: bytes) -> dict:
    out.mkdir(parents=True, exist_ok=True)
    (out / name).write_bytes(data)
    digest = "sha256:" + hashlib.sha256(data).hexdigest()
    (out / (name + ".digest")).write_text(digest + "\n")
    return {"digest": digest, "size": len(data)}


def read_layers(text: str) -> list[dict]:
    rows = []
    for line in text.splitlines():
        if not line.strip():
            continue
        name, diff_id, digest, size, check = line.split("\t")[:5]
        if check != "0":
            raise ValueError(f"layer {name}: its check failed ({check}); no image of a failed capture")
        for d in (diff_id, digest):
            if not (d.startswith("sha256:") and len(d) == 71):
                raise ValueError(f"layer {name}: {d!r} is not a sha256 digest")
        rows.append({"name": name, "diff_id": diff_id, "digest": digest, "size": int(size)})
    if not rows or rows[-1]["name"] != "boot":
        raise ValueError("the layer table must end with boot")
    return rows


def image(layers: list[dict], record: dict, out: pathlib.Path) -> dict:
    arch = ARCH.get(record["arch"])
    if arch is None:
        raise ValueError(f"no OCI arch for {record['arch']!r}")
    notes = {
        "org.opencontainers.image.source": SOURCE,
        "org.opencontainers.image.title": "hosted snapshot of " + record.get("label", ""),
        "dev.podrunner.hosted.image-os": record["image_os"],
        "dev.podrunner.hosted.image-version": record["image_version"],
        "dev.podrunner.hosted.kernel": record["kernel"],
    }
    config = {
        "architecture": arch,
        "os": "linux",
        # The init of the image boots the unit (`--podvm-init=image`).
        "config": {"Entrypoint": ["/sbin/init"], "Labels": notes},
        "rootfs": {"type": "layers", "diff_ids": [lay["diff_id"] for lay in layers]},
        "history": [{"created_by": "podrunner hosted capture: " + lay["name"]} for lay in layers],
    }
    c = put(out, "config.json", canon(config))
    manifest = {
        "schemaVersion": 2,
        "mediaType": MANIFEST,
        "config": {"mediaType": CONFIG, **c},
        "layers": [
            {"mediaType": LAYER, "digest": lay["digest"], "size": lay["size"],
             "annotations": {"dev.podrunner.hosted.layer": lay["name"]}}
            for lay in layers
        ],
        "annotations": notes,
    }
    m = put(out, "manifest.json", canon(manifest))
    return {"mediaType": MANIFEST, **m, "platform": {"architecture": arch, "os": "linux"}}


def index(entries: list[dict], out: pathlib.Path) -> dict:
    arches = [e["platform"]["architecture"] for e in entries]
    if len(set(arches)) != len(arches):
        raise ValueError(f"two manifests of one arch: {arches}")
    return put(out, "index.json", canon({"schemaVersion": 2, "mediaType": INDEX, "manifests": entries}))


def self_test() -> int:
    import tempfile

    bad = []
    record = {"arch": "x86_64", "image_os": "ubuntu24", "image_version": "20261111.370.1",
              "kernel": "7.0.0-1017-azure", "label": "ubuntu-24.04"}
    d1, d2 = "sha256:" + "a" * 64, "sha256:" + "b" * 64
    table = f"00-jvm\t{d1}\t{d2}\t10\t0\nboot\t{d2}\t{d1}\t20\t0\n"
    with tempfile.TemporaryDirectory() as t:
        out = pathlib.Path(t)
        e1 = image(read_layers(table), record, out / "a")
        e2 = image(read_layers(table), record, out / "b")
        if e1 != e2:
            bad.append("the same capture gives two digests")
        m = json.loads((out / "a" / "manifest.json").read_text())
        if [x["mediaType"] for x in m["layers"]] != [LAYER, LAYER] or m["layers"][1]["size"] != 20:
            bad.append(f"the layers: {m['layers']}")
        c = json.loads((out / "a" / "config.json").read_text())
        if c["rootfs"]["diff_ids"] != [d1, d2] or c["architecture"] != "amd64":
            bad.append(f"the config: {c}")
        if m["config"]["digest"] != (out / "a" / "config.json.digest").read_text().strip():
            bad.append("the digest of the config")
        arm = image(read_layers(table), {**record, "arch": "aarch64"}, out / "c")
        index([e1, arm], out / "i")
        ix = json.loads((out / "i" / "index.json").read_text())
        if [x["platform"]["architecture"] for x in ix["manifests"]] != ["amd64", "arm64"]:
            bad.append(f"the index: {ix}")
        for wrong, why in ((table.replace("\t0\n", "\t1\n", 1), "a failed check"),
                           (f"00-jvm\t{d1}\t{d2}\t10\t0\n", "no boot"),
                           (table.replace(d1, "sha256:short", 1), "a bad digest")):
            try:
                read_layers(wrong)
                bad.append(f"{why} is taken")
            except ValueError:
                pass
        try:
            index([e1, e1], out / "j")
            bad.append("two manifests of one arch are taken")
        except ValueError:
            pass
    for b in bad:
        print(f"self-test: {b}")
    if not bad:
        print("oci.py: self-test passes")
    return 1 if bad else 0


def main() -> int:
    a = sys.argv[1:]
    if a == ["--self-test"]:
        return self_test()
    if a[:1] == ["image"] and len(a) == 7 and a[1] == "--layers" and a[3] == "--record" and a[5] == "--out":
        e = image(read_layers(pathlib.Path(a[2]).read_text()), json.loads(pathlib.Path(a[4]).read_text()),
                  pathlib.Path(a[6]))
        (pathlib.Path(a[6]) / "entry.json").write_text(json.dumps(e) + "\n")
        print(e["digest"])
        return 0
    if a[:2] == ["index", "--out"] and len(a) > 3:
        entries = [json.loads(pathlib.Path(p).read_text()) for p in a[3:]]
        print(index(entries, pathlib.Path(a[2]))["digest"])
        return 0
    print(__doc__.strip().splitlines()[2], file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
