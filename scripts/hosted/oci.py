#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The OCI config, manifest and index of the hosted snapshot (T-555).

    python3 -I scripts/hosted/oci.py image --layers LAYERS.TSV --record SNAPSHOT.JSON --out DIR
    python3 -I scripts/hosted/oci.py append --manifest FILE --config FILE --layer ROW.TSV --out DIR [KEY=VALUE...]
    python3 -I scripts/hosted/oci.py entry --manifest FILE --arch amd64|arm64 > ENTRY.JSON
    python3 -I scripts/hosted/oci.py index --out DIR ENTRY.JSON...
    python3 -I scripts/hosted/oci.py --self-test

`image` reads the layer table of a capture (`capture.sh`: name, diff id,
digest, size, check) and its record, and writes DIR/config.json and
DIR/manifest.json, each with the digest of its exact bytes in a file
beside it (`.digest`). The layers are `tar+zstd`; `boot` is the last. The
annotations name the hosted image (ImageOS, ImageVersion, the kernel) and
the source of podrunner. `append` writes the image of a published base
(the exact bytes of its manifest and config) with one more layer on top,
the runner layer of T-541 (ROW.TSV: one row of the layer table), with
each KEY=VALUE as a label and an annotation. `entry` makes the index entry of a
manifest read back from the registry (its exact bytes), and `index` writes
one multi-arch index of the entries. The bytes are canonical (sorted keys,
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


def read_row(line: str) -> dict:
    """One row of a layer table: name, diff id, digest, size, check."""
    name, diff_id, digest, size, check = line.split("\t")[:5]
    if check != "0":
        raise ValueError(f"layer {name}: its check failed ({check}); no image of a failed capture")
    for d in (diff_id, digest):
        if not (d.startswith("sha256:") and len(d) == 71):
            raise ValueError(f"layer {name}: {d!r} is not a sha256 digest")
    return {"name": name, "diff_id": diff_id, "digest": digest, "size": int(size)}


def read_layers(text: str) -> list[dict]:
    rows = [read_row(line) for line in text.splitlines() if line.strip()]
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


def append(base_manifest: bytes, base_config: bytes, layer: dict, labels: dict, out: pathlib.Path) -> dict:
    """The image of BASE (the exact bytes of its manifest and config) with
    LAYER on top (T-541): one more diff id, one more blob, no build. Each
    other blob of the base stays shared. LABELS go into the labels of the
    config and the annotations of the manifest; the manifest names its base."""
    m, c = json.loads(base_manifest), json.loads(base_config)
    if m.get("mediaType") != MANIFEST:
        raise ValueError(f"the base is not an OCI image manifest: {m.get('mediaType')!r}")
    if m["config"]["digest"] != "sha256:" + hashlib.sha256(base_config).hexdigest():
        raise ValueError("the config is not the one of the base manifest")
    if len(m["layers"]) != len(c["rootfs"]["diff_ids"]):
        raise ValueError(f"the base has {len(m['layers'])} layers and {len(c['rootfs']['diff_ids'])} diff ids")
    names = [lay.get("annotations", {}).get("dev.podrunner.hosted.layer") for lay in m["layers"]]
    if layer["name"] in names:
        raise ValueError(f"the base has a layer {layer['name']} already")
    c["rootfs"]["diff_ids"].append(layer["diff_id"])
    c.setdefault("history", []).append({"created_by": "podrunner hosted layer: " + layer["name"]})
    c.setdefault("config", {}).setdefault("Labels", {}).update(labels)
    cfg = put(out, "config.json", canon(c))
    m["config"] = {"mediaType": CONFIG, **cfg}
    m["layers"].append({"mediaType": LAYER, "digest": layer["digest"], "size": layer["size"],
                        "annotations": {"dev.podrunner.hosted.layer": layer["name"]}})
    base = "sha256:" + hashlib.sha256(base_manifest).hexdigest()
    m["annotations"] = {**m.get("annotations", {}), **labels, "dev.podrunner.hosted.base": base}
    man = put(out, "manifest.json", canon(m))
    return {"mediaType": MANIFEST, **man, "platform": {"architecture": c["architecture"], "os": "linux"}}


def entry(data: bytes, arch: str) -> dict:
    """The index entry of the manifest DATA (its exact bytes) for ARCH."""
    if arch not in ARCH.values():
        raise ValueError(f"no OCI arch {arch!r}")
    m = json.loads(data)
    if m.get("mediaType") != MANIFEST:
        raise ValueError(f"not an OCI image manifest: {m.get('mediaType')!r}")
    return {"mediaType": MANIFEST, "digest": "sha256:" + hashlib.sha256(data).hexdigest(), "size": len(data),
            "platform": {"architecture": arch, "os": "linux"}}


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
        back = entry((out / "c" / "manifest.json").read_bytes(), "arm64")
        if back != arm:
            bad.append(f"the entry of a manifest read back: {back} {arm}")
        try:
            entry(b'{"mediaType":"application/vnd.docker.distribution.manifest.v2+json"}', "amd64")
            bad.append("a manifest of another type is taken")
        except ValueError:
            pass
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
        # T-541: the runner layer on top of the snapshot.
        d3, d4 = "sha256:" + "c" * 64, "sha256:" + "d" * 64
        base_m, base_c = (out / "a" / "manifest.json").read_bytes(), (out / "a" / "config.json").read_bytes()
        row = read_row(f"runner\t{d3}\t{d4}\t30\t0")
        labels = {"dev.podrunner.runner-version": "9.9.9"}
        r = append(base_m, base_c, row, labels, out / "r")
        if append(base_m, base_c, row, labels, out / "r2") != r:
            bad.append("the same append gives two digests")
        rm = json.loads((out / "r" / "manifest.json").read_text())
        rc = json.loads((out / "r" / "config.json").read_text())
        if rc["rootfs"]["diff_ids"] != [d1, d2, d3] or len(rc["history"]) != 3:
            bad.append(f"the config of the runner image: {rc['rootfs']}")
        if [x["digest"] for x in rm["layers"]] != [d2, d1, d4] or rm["layers"][2]["size"] != 30:
            bad.append(f"the layers of the runner image: {rm['layers']}")
        if rm["layers"][:2] != m["layers"]:
            bad.append("the layers of the base changed")
        if rm["layers"][2]["annotations"] != {"dev.podrunner.hosted.layer": "runner"}:
            bad.append(f"the name of the runner layer: {rm['layers'][2]}")
        if rc["config"]["Entrypoint"] != ["/sbin/init"] or rc["config"]["Labels"].get(
                "dev.podrunner.runner-version") != "9.9.9" or rc["config"]["Labels"].get(
                "dev.podrunner.hosted.image-os") != "ubuntu24":
            bad.append(f"the labels of the runner image: {rc['config']}")
        if rm["annotations"].get("dev.podrunner.hosted.base") != e1["digest"]:
            bad.append(f"the runner image names no base: {rm['annotations']}")
        if r["digest"] != (out / "r" / "manifest.json.digest").read_text().strip() or r["digest"] == e1["digest"]:
            bad.append("the digest of the runner manifest")
        if rm["config"]["digest"] != (out / "r" / "config.json.digest").read_text().strip():
            bad.append("the config digest of the runner manifest")
        for why, args in (("a second runner layer", ((out / "r" / "manifest.json").read_bytes(),
                                                     (out / "r" / "config.json").read_bytes())),
                          ("a config of another manifest", (base_m, (out / "c" / "config.json").read_bytes()))):
            try:
                append(*args, row, labels, out / "x")
                bad.append(f"{why} is taken")
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
    if a[:1] == ["entry"] and len(a) == 5 and a[1] == "--manifest" and a[3] == "--arch":
        print(json.dumps(entry(pathlib.Path(a[2]).read_bytes(), a[4])))
        return 0
    if a[:1] == ["append"] and len(a) >= 9 and a[1:8:2] == ["--manifest", "--config", "--layer", "--out"]:
        labels = {}
        for kv in a[9:]:
            k, eq, v = kv.partition("=")
            if not (eq and k and v):
                raise ValueError(f"a label is KEY=VALUE, not {kv!r}")
            labels[k] = v
        rows = [line for line in pathlib.Path(a[6]).read_text().splitlines() if line.strip()]
        if len(rows) != 1:
            raise ValueError(f"{a[6]}: one row, not {len(rows)}")
        e = append(pathlib.Path(a[2]).read_bytes(), pathlib.Path(a[4]).read_bytes(), read_row(rows[0]), labels,
                   pathlib.Path(a[8]))
        (pathlib.Path(a[8]) / "entry.json").write_text(json.dumps(e) + "\n")
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
