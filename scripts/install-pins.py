#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""Write or check the block of pins of scripts/install.sh (T-202).

The block comes from images/podvm.toml, images/podnet.toml and
images/runner/runner.toml. A release adds its own version, the SHA-256 of
each podrunner binary and of each runner image archive, the manifest
digest of each image (from index.json in the archive) and its unpacked
size (from its layers).

  install-pins.py --check FILE
      exit 1 when the block of FILE is not the block of the tree (no
      release values)
  install-pins.py [--images DIR] [--release VERSION --assets DIR] FILE [--out OUT]
      write the block into FILE (or into a copy OUT)
  install-pins.py [--images DIR] --release VERSION --assets DIR --manifest OUT
      write the manifest of the release that a machine reads (T-499): the
      same values as JSON
"""

import argparse
import datetime
import hashlib
import json
import pathlib
import posixpath
import re
import sys
import tarfile
import tomllib

ARCHES = ("x86_64", "aarch64")
BEGIN = "# BEGIN PINS (scripts/install-pins.py writes this block; do not edit it)"
END = "# END PINS"
HEX = re.compile(r"^[0-9a-f]{64}$")


def toml_pin(path: pathlib.Path) -> tuple[str, dict[str, str]]:
    d = tomllib.loads(path.read_text())
    for a in ARCHES:
        if not HEX.match(d["sha256"][a]):
            sys.exit(f"{path}: the sha256 of {a} is not 64 hex digits")
    # T-553: the index of the same release on ghcr.io, when the project
    # publishes one. A new version with the reference of the old one fails.
    ref = d.get("ghcr", {}).get("reference")
    form = rf"^ghcr\.io/podutils/[a-z0-9-]+:v{re.escape(d['version'])}@sha256:[0-9a-f]{{64}}$"
    if ref is not None and not re.match(form, ref):
        sys.exit(f"{path}: [ghcr] reference {ref!r} is not ghcr.io/podutils/NAME:v{d['version']}@sha256:HEX")
    return d["version"], d["sha256"]


def sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def image_digest(path: pathlib.Path) -> str:
    """The manifest digest of the archive that `podvm load -i` takes."""
    with tarfile.open(path) as t:
        names = [m for m in t.getnames() if m.lstrip("./") == "index.json"]
        if not names:
            sys.exit(f"{path}: no index.json: not an OCI layout")
        index = json.load(t.extractfile(names[0]))
    m = index["manifests"]
    if len(m) != 1:
        sys.exit(f"{path}: {len(m)} manifests in index.json, not one")
    if "org.opencontainers.image.ref.name" not in m[0].get("annotations", {}):
        sys.exit(f"{path}: no ref name in index.json: podvm load needs one")
    digest = m[0]["digest"]
    if not re.match(r"^sha256:[0-9a-f]{64}$", digest):
        sys.exit(f"{path}: the digest {digest!r} is not sha256:HEX")
    return digest


def values(images: pathlib.Path, assets: pathlib.Path | None) -> tuple[dict, dict[str, str]]:
    """The pins of the tree, and the values of the files of a release."""
    pins = {
        "podvm": toml_pin(images / "podvm.toml"),
        "podnet": toml_pin(images / "podnet.toml"),
        "runner": toml_pin(images / "runner" / "runner.toml"),
    }
    v: dict[str, str] = {}
    for a in ARCHES:
        v[f"podrunner_{a}"] = ""
        v[f"image_{a}"] = v[f"image_digest_{a}"] = v[f"image_unpacked_{a}"] = ""
        v[f"imagefull_{a}"] = v[f"imagefull_digest_{a}"] = v[f"imagefull_unpacked_{a}"] = ""
        if assets is None:
            continue
        exe = assets / f"podrunner-{a}-unknown-linux-musl"
        base = assets / f"runner-{a}.tar"
        for f in (exe, base):
            if not f.is_file():
                sys.exit(f"{f}: missing: a release needs it")
        v[f"podrunner_{a}"] = sha256(exe)
        v[f"image_{a}"], v[f"image_digest_{a}"] = sha256(base), image_digest(base)
        v[f"image_unpacked_{a}"] = str(image_unpacked(base))
        full = assets / f"runner-full-{a}.tar"
        if full.is_file():
            v[f"imagefull_{a}"], v[f"imagefull_digest_{a}"] = sha256(full), image_digest(full)
            v[f"imagefull_unpacked_{a}"] = str(image_unpacked(full))
    return pins, v


def manifest(images: pathlib.Path, release: str, assets: pathlib.Path) -> str:
    """The manifest of a release, as JSON (T-499, githome's G-23)."""
    pins, v = values(images, assets)

    def image(kind: str, a: str) -> dict | None:
        name = "runner" if kind == "image" else "runner-full"
        if not v[f"{kind}_{a}"]:
            return None
        f = f"{name}-{a}.tar"
        return {
            "file": f,
            "sha256": v[f"{kind}_{a}"],
            "digest": v[f"{kind}_digest_{a}"],
            **image_metadata(assets / f),
            "unpacked_bytes": int(v[f"{kind}_unpacked_{a}"]),
        }

    doc = {
        "schema": 1,
        "podrunner": {
            "version": release,
            "files": {a: {"file": f"podrunner-{a}-unknown-linux-musl", "sha256": v[f"podrunner_{a}"]} for a in ARCHES},
        },
        "runner": {
            "version": pins["runner"][0],
            "tarball_sha256": pins["runner"][1],
            "images": {a: {"base": image("image", a), "full": image("imagefull", a)} for a in ARCHES},
        },
        "podvm": {"version": pins["podvm"][0], "sha256": pins["podvm"][1]},
        "podnet": {"version": pins["podnet"][0], "sha256": pins["podnet"][1]},
    }
    return json.dumps(doc, indent=2, sort_keys=True) + "\n"


def image_metadata(path: pathlib.Path) -> dict:
    """The build time and optional Docker engine declaration from OCI config."""
    with tarfile.open(path) as t:
        names = {m.lstrip("./"): m for m in t.getnames()}

        def blob(digest: str) -> dict | None:
            name = names.get("blobs/" + digest.replace(":", "/"))
            f = t.extractfile(name) if name else None
            return json.load(f) if f else None

        index = json.load(t.extractfile(names["index.json"]))
        manifest = blob(index["manifests"][0]["digest"]) if index.get("manifests") else None
        config = blob(manifest["config"]["digest"]) if manifest and "config" in manifest else None
    config = config or {}
    result = {"built_at_s": None}
    created = config.get("created")
    if isinstance(created, str):
        try:
            result["built_at_s"] = int(datetime.datetime.fromisoformat(created.replace("Z", "+00:00")).timestamp())
        except ValueError:
            pass
    labels = (config.get("config") or {}).get("Labels") or {}
    if labels.get("io.podutils.podrunner.engine") == "1":
        result["container_engine"] = True
    return result


def image_unpacked(path: pathlib.Path) -> int:
    """The bytes of the regular files of the image of an archive, after the
    whiteouts of its layers: the `unpacked_bytes` of a pin of it (T-572).
    A hard link counts once, as `du` counts it."""
    files: dict[str, int] = {}
    with tarfile.open(path) as t:
        names = {m.lstrip("./"): m for m in t.getnames()}

        def blob(digest: str):
            name = names.get("blobs/" + digest.replace(":", "/"))
            if name is None:
                sys.exit(f"{path}: no blob {digest}")
            return t.extractfile(name)

        index = json.load(t.extractfile(names["index.json"]))
        manifest = json.load(blob(index["manifests"][0]["digest"]))
        for layer in manifest.get("layers", []):
            media = layer.get("mediaType", "")
            if media.endswith("gzip"):
                mode = "r|gz"
            elif media.endswith("tar"):
                mode = "r|"
            else:
                sys.exit(f"{path}: the layer type {media!r} is not tar or tar+gzip")
            put: dict[str, int | None] = {}
            gone: list[str] = []
            with tarfile.open(fileobj=blob(layer["digest"]), mode=mode) as lt:
                for m in lt:
                    name = posixpath.normpath("/" + m.name).lstrip("/")
                    if not name or name == ".":
                        continue
                    where, base = posixpath.split(name)
                    if base == ".wh..wh..opq":
                        gone.append(where + "/" if where else "")
                    elif base.startswith(".wh."):
                        gone.append(posixpath.join(where, base[4:]))
                    else:
                        put[name] = m.size if m.isreg() else None
            # A whiteout hides the lower layers only, so it goes first.
            for g in gone:
                for k in [k for k in files if k == g or k.startswith(g.rstrip("/") + "/") or g == ""]:
                    del files[k]
            for k, size in put.items():
                if size is None:
                    files.pop(k, None)
                else:
                    files[k] = size
    return sum(files.values())


def block(images: pathlib.Path, release: str, assets: pathlib.Path | None) -> str:
    pins, v = values(images, assets)
    (podvm_v, podvm), (podnet_v, podnet), (runner_v, runner) = pins["podvm"], pins["podnet"], pins["runner"]
    lines = [
        BEGIN,
        f'PODRUNNER_VERSION="{release}"',
        f'PODVM_VERSION="{podvm_v}"',
        f'PODNET_VERSION="{podnet_v}"',
        f'RUNNER_VERSION="{runner_v}"',
    ]
    lines += [f'SUM_podrunner_{a}="{v[f"podrunner_{a}"]}"' for a in ARCHES]
    lines += [f'SUM_podvm_{a}="{podvm[a]}"' for a in ARCHES]
    lines += [f'SUM_podnet_{a}="{podnet[a]}"' for a in ARCHES]
    lines += [f'SUM_runner_{a}="{runner[a]}"' for a in ARCHES]
    lines += [f'SUM_image_{a}="{v[f"image_{a}"]}"' for a in ARCHES]
    lines += [f'DIGEST_image_{a}="{v[f"image_digest_{a}"]}"' for a in ARCHES]
    lines += [f'SUM_imagefull_{a}="{v[f"imagefull_{a}"]}"' for a in ARCHES]
    lines += [f'DIGEST_imagefull_{a}="{v[f"imagefull_digest_{a}"]}"' for a in ARCHES]
    lines += [f'UNPACKED_image_{a}="{v[f"image_unpacked_{a}"]}"' for a in ARCHES]
    lines += [f'UNPACKED_imagefull_{a}="{v[f"imagefull_unpacked_{a}"]}"' for a in ARCHES]
    lines.append(END)
    return "\n".join(lines) + "\n"


def split(text: str, name: str) -> tuple[str, str, str]:
    start, end = text.find(BEGIN + "\n"), text.find(END + "\n")
    if start < 0 or end < start:
        sys.exit(f"{name}: no block of pins ({BEGIN!r} ... {END!r})")
    end += len(END) + 1
    return text[:start], text[start:end], text[end:]


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("file", nargs="?")
    p.add_argument("--check", action="store_true")
    p.add_argument("--images", default="images")
    p.add_argument("--release", default="")
    p.add_argument("--assets")
    p.add_argument("--out")
    p.add_argument("--manifest")
    o = p.parse_args()
    if bool(o.release) != bool(o.assets):
        sys.exit("--release and --assets go together")
    if o.check and o.release:
        sys.exit("--check takes no release")
    if o.release and not re.match(r"^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$", o.release):
        sys.exit(f"--release {o.release!r} is not a version")
    if o.manifest:
        if not o.release or o.file or o.check:
            sys.exit("--manifest takes --release and --assets, and no FILE")
        pathlib.Path(o.manifest).write_text(manifest(pathlib.Path(o.images), o.release, pathlib.Path(o.assets)))
        print(f"{o.manifest}: the manifest is written")
        return 0
    if not o.file:
        sys.exit("a FILE is necessary")
    text = pathlib.Path(o.file).read_text()
    head, old, tail = split(text, o.file)
    new = block(pathlib.Path(o.images), o.release, pathlib.Path(o.assets) if o.assets else None)
    if o.check:
        if old != new:
            print(f"{o.file}: the block of pins is not the block of {o.images}: run python3 scripts/install-pins.py {o.file}")
            for a, b in zip(old.splitlines(), new.splitlines()):
                if a != b:
                    print(f"  have {a}\n  want {b}")
            return 1
        print(f"{o.file}: the pins agree with {o.images}")
        return 0
    out = pathlib.Path(o.out or o.file)
    out.write_text(head + new + tail)
    out.chmod(0o755)
    print(f"{out}: the block of pins is written")
    return 0


if __name__ == "__main__":
    sys.exit(main())
