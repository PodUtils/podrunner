#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The core slice of a published hosted snapshot (T-556).

    python3 -I scripts/hosted/slice.py --digest sha256:HEX --ref NAME --out DIR [--all]
        [--keep GLOB]... [--layers FILE]
    python3 -I scripts/hosted/slice.py --self-test

The full snapshot is about 55 GB on disk; a hosted runner cannot boot it
under incus (T-557). The slice keeps the layers that boot the system (the
base, the libraries and the shared files of the distribution, snapd and
`boot`, and the runner layer of a runner image, T-541) and leaves out
the trees of the tools (the Android SDK, .NET, the JVMs, the tool cache,
the other trees of /opt and /usr/local, the stores of the engines). It reads the manifest and the config of DIGEST from
ghcr.io/podutils/podrunner-hosted with no login (the package is public),
downloads each kept layer and checks its digest, and writes an OCI layout
in DIR whose config names only the kept layers, with the ref NAME, as
`podvm load -i` takes it after a tar of its files. With `--all` the
layout keeps each layer: the full snapshot, for the measure of the disk
(hosted-measure.yml, T-557). Each `--keep GLOB` keeps the layers whose
name (with no NN- prefix) matches GLOB too, for example the tool cache of
one Python (`toolcache-3.12.*`) for the canned job of hosted-boot.yml
(T-541). A GLOB that matches no layer is a note on stderr, not an error:
the caller checks the names in FILE (`--layers`), one kept name on each
line, with no NN- prefix.
"""

import argparse
import fnmatch
import hashlib
import json
import pathlib
import sys
import urllib.request

REGISTRY = "https://ghcr.io"
NAME = "podutils/podrunner-hosted"
MANIFEST = "application/vnd.oci.image.manifest.v1+json"
CONFIG = "application/vnd.oci.image.config.v1+json"

# The layers of the slice, by the name that the capture gave each one, and
# the runner layer of a runner image (T-541, oci.py append).
KEEP = ("base", "boot", "usr-share", "usr-lib", "usr-lib-arch", "snapd", "runner")


def bare(layer_name: str) -> str:
    # A capture names a layer NN-NAME; boot has no number.
    return layer_name.split("-", 1)[1] if layer_name[:2].isdigit() else layer_name


def kept(layer_name: str, extra: tuple[str, ...] = ()) -> bool:
    b = bare(layer_name)
    return b in KEEP or any(fnmatch.fnmatchcase(b, g) for g in extra)


def sha(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


class Registry:
    def __init__(self):
        q = f"{REGISTRY}/token?service=ghcr.io&scope=repository:{NAME}:pull"
        with urllib.request.urlopen(q, timeout=60) as r:
            self.token = json.load(r)["token"]

    def get(self, path: str, accept: str | None = None, out: pathlib.Path | None = None) -> bytes:
        req = urllib.request.Request(f"{REGISTRY}/v2/{NAME}/{path}", headers={"Authorization": f"Bearer {self.token}"})
        if accept:
            req.add_header("Accept", accept)
        with urllib.request.urlopen(req, timeout=600) as r:
            if out is None:
                return r.read()
            h = hashlib.sha256()
            with open(out, "wb") as f:
                while b := r.read(1 << 20):
                    h.update(b)
                    f.write(b)
            return ("sha256:" + h.hexdigest()).encode()


def slice_image(manifest: dict, config: dict, keep_all: bool = False,
                extra: tuple[str, ...] = ()) -> tuple[dict, dict]:
    """The manifest and the config of the slice (the layers to fetch are
    those of the new manifest). KEEP_ALL keeps each layer; EXTRA holds
    globs of more names to keep."""
    layers = manifest["layers"]
    diff_ids = config["rootfs"]["diff_ids"]
    history = config.get("history", [])
    if len(layers) != len(diff_ids):
        raise ValueError(f"{len(layers)} layers and {len(diff_ids)} diff ids")
    keep = [i for i, lay in enumerate(layers)
            if keep_all or kept(lay.get("annotations", {}).get("dev.podrunner.hosted.layer", ""), extra)]
    full = [layers[i].get("annotations", {}).get("dev.podrunner.hosted.layer", "") for i in keep]
    for want in ("base", "boot"):
        if not any(n == want or n.endswith("-" + want) for n in full):
            raise ValueError(f"no layer {want} in the manifest")
    new_config = dict(config)
    new_config["rootfs"] = {"type": "layers", "diff_ids": [diff_ids[i] for i in keep]}
    if len(history) == len(layers):
        new_config["history"] = [history[i] for i in keep]
    new_manifest = dict(manifest)
    new_manifest["layers"] = [layers[i] for i in keep]
    return new_manifest, new_config


def names(manifest: dict) -> list[str]:
    """The name of each layer of MANIFEST, with no NN- prefix."""
    return [bare(lay.get("annotations", {}).get("dev.podrunner.hosted.layer", "")) for lay in manifest["layers"]]


def unmatched(manifest: dict, extra: tuple[str, ...]) -> list[str]:
    """Each glob of EXTRA that matches no layer of MANIFEST."""
    have = names(manifest)
    return [g for g in extra if not any(fnmatch.fnmatchcase(n, g) for n in have)]


def write_layout(out: pathlib.Path, manifest: dict, config: dict, ref: str, fetch) -> str:
    """Write the OCI layout; FETCH(digest, path) downloads one layer and
    returns the digest of its bytes."""
    blobs = out / "blobs" / "sha256"
    blobs.mkdir(parents=True, exist_ok=True)
    cfg = json.dumps(config, sort_keys=True, separators=(",", ":")).encode()
    (blobs / sha(cfg)[7:]).write_bytes(cfg)
    manifest = dict(manifest)
    manifest["config"] = {"mediaType": CONFIG, "digest": sha(cfg), "size": len(cfg)}
    for lay in manifest["layers"]:
        path = blobs / lay["digest"][7:]
        got = fetch(lay["digest"], path)
        if got != lay["digest"]:
            raise ValueError(f"the layer {lay['digest']}: its bytes give {got}")
    man = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
    (blobs / sha(man)[7:]).write_bytes(man)
    (out / "oci-layout").write_text('{"imageLayoutVersion":"1.0.0"}')
    index = {"schemaVersion": 2, "manifests": [{
        "mediaType": MANIFEST, "digest": sha(man), "size": len(man),
        "annotations": {"org.opencontainers.image.ref.name": ref}}]}
    (out / "index.json").write_text(json.dumps(index))
    return sha(man)


def self_test() -> int:
    import tempfile

    bad = []
    fixture = ["00-android-ndk", "21-toolcache-3.12.1", "22-toolcache-3.13.2", "56-opt-hostedtoolcache",
              "79-usr-share", "80-usr-lib-arch", "81-base", "boot", "runner"]
    data = {n: n.encode() * 3 for n in fixture}
    manifest = {"schemaVersion": 2, "mediaType": MANIFEST, "config": {},
                "layers": [{"mediaType": "application/vnd.oci.image.layer.v1.tar+zstd", "digest": sha(data[n]),
                            "size": len(data[n]), "annotations": {"dev.podrunner.hosted.layer": n}} for n in fixture]}
    config = {"architecture": "amd64", "os": "linux",
              "rootfs": {"type": "layers", "diff_ids": ["sha256:" + str(i) * 64 for i in range(len(fixture))]},
              "history": [{"created_by": n} for n in fixture]}
    m, c = slice_image(manifest, config)
    got = [lay["annotations"]["dev.podrunner.hosted.layer"] for lay in m["layers"]]
    if got != ["79-usr-share", "80-usr-lib-arch", "81-base", "boot", "runner"]:
        bad.append(f"the kept layers: {got}")
    if c["rootfs"]["diff_ids"] != ["sha256:" + str(i) * 64 for i in (4, 5, 6, 7, 8)] or len(c["history"]) != 5:
        bad.append(f"the config: {c['rootfs']}")
    # T-541: --keep adds the tool cache of one Python and its parent layer,
    # in the order of the manifest, and no other version.
    keep = ("opt-hostedtoolcache", "toolcache-3.12.*")
    m, c = slice_image(manifest, config, extra=keep)
    if names(m) != ["toolcache-3.12.1", "opt-hostedtoolcache", "usr-share", "usr-lib-arch", "base", "boot", "runner"]:
        bad.append(f"the kept layers with --keep: {names(m)}")
    if c["rootfs"]["diff_ids"] != ["sha256:" + str(i) * 64 for i in (1, 3, 4, 5, 6, 7, 8)]:
        bad.append(f"the config with --keep: {c['rootfs']}")
    if unmatched(manifest, keep) or unmatched(manifest, ("toolcache-3.9.*", "base")) != ["toolcache-3.9.*"]:
        bad.append("the globs that match no layer")
    # A glob does not match a part of a name, and no case is ignored.
    m, _ = slice_image(manifest, config, extra=("3.12.1", "TOOLCACHE-3.13.2"))
    if "toolcache-3.12.1" in names(m) or "toolcache-3.13.2" in names(m):
        bad.append(f"a part of a name or another case is kept: {names(m)}")
    m, c = slice_image(manifest, config, keep_all=True)
    if len(m["layers"]) != len(fixture) or c["rootfs"]["diff_ids"] != config["rootfs"]["diff_ids"]:
        bad.append(f"--all keeps {len(m['layers'])} of {len(fixture)} layers")
    m, c = slice_image(manifest, config)
    by_digest = {sha(v): v for v in data.values()}

    def fetch(d, path):
        path.write_bytes(by_digest[d])
        return sha(path.read_bytes())

    with tempfile.TemporaryDirectory() as t:
        out = pathlib.Path(t)
        dig = write_layout(out, m, c, "hosted:core", fetch)
        idx = json.loads((out / "index.json").read_text())
        if idx["manifests"][0]["digest"] != dig or not (out / "blobs/sha256" / dig[7:]).is_file():
            bad.append("the layout")
        mm = json.loads((out / "blobs/sha256" / dig[7:]).read_text())
        if sha((out / "blobs/sha256" / mm["config"]["digest"][7:]).read_bytes()) != mm["config"]["digest"]:
            bad.append("the digest of the new config")

        def wrong(d, path):
            path.write_bytes(b"other")
            return sha(b"other")

        try:
            write_layout(out / "x", m, c, "hosted:core", wrong)
            bad.append("a layer with other bytes is taken")
        except ValueError:
            pass
    try:
        slice_image({**manifest, "layers": manifest["layers"][:2]}, {**config, "rootfs": {"diff_ids": ["a", "b"]}})
        bad.append("a manifest with no base is taken")
    except ValueError:
        pass
    for b in bad:
        print(f"self-test: {b}")
    if not bad:
        print("slice.py: self-test passes")
    return 1 if bad else 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    ap = argparse.ArgumentParser()
    ap.add_argument("--digest", required=True)
    ap.add_argument("--ref", required=True)
    ap.add_argument("--out", type=pathlib.Path, required=True)
    ap.add_argument("--all", action="store_true", help="keep each layer (the full snapshot)")
    ap.add_argument("--keep", action="append", default=[], metavar="GLOB",
                    help="keep the layers whose name (no NN- prefix) matches GLOB too")
    ap.add_argument("--layers", type=pathlib.Path, metavar="FILE",
                    help="write the name of each kept layer (no NN- prefix), one on each line")
    a = ap.parse_args()
    reg = Registry()
    raw = reg.get(f"manifests/{a.digest}", MANIFEST)
    if sha(raw) != a.digest:
        raise ValueError(f"the manifest gives {sha(raw)}, not {a.digest}")
    manifest = json.loads(raw)
    config_raw = reg.get(f"blobs/{manifest['config']['digest']}")
    if sha(config_raw) != manifest["config"]["digest"]:
        raise ValueError("the config has other bytes than its digest")
    extra = tuple(a.keep)
    m, c = slice_image(manifest, json.loads(config_raw), keep_all=a.all, extra=extra)
    for g in unmatched(manifest, extra):
        print(f"slice: --keep {g}: no layer of the manifest matches it", file=sys.stderr)
    size = sum(lay["size"] for lay in m["layers"])
    print(f"slice: {len(m['layers'])} of {len(manifest['layers'])} layers, {size >> 20} MiB", file=sys.stderr)
    if a.layers:
        a.layers.write_text("".join(n + "\n" for n in names(m)))
    dig = write_layout(a.out, m, c, a.ref, lambda d, p: reg.get(f"blobs/{d}", out=p).decode())
    print(dig)
    return 0


if __name__ == "__main__":
    sys.exit(main())
