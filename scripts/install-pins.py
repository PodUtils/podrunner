#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""Write or check the block of pins of scripts/install.sh (T-202).

The block comes from images/podvm.toml, images/podnet.toml and
images/runner/runner.toml. A release adds its own version, the SHA-256 of
each podrunner binary and of each runner image archive, and the manifest
digest of each image (from index.json in the archive).

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
import hashlib
import json
import pathlib
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
        v[f"image_{a}"] = v[f"image_digest_{a}"] = ""
        v[f"imagefull_{a}"] = v[f"imagefull_digest_{a}"] = ""
        if assets is None:
            continue
        exe = assets / f"podrunner-{a}-unknown-linux-musl"
        base = assets / f"runner-{a}.tar"
        for f in (exe, base):
            if not f.is_file():
                sys.exit(f"{f}: missing: a release needs it")
        v[f"podrunner_{a}"] = sha256(exe)
        v[f"image_{a}"], v[f"image_digest_{a}"] = sha256(base), image_digest(base)
        full = assets / f"runner-full-{a}.tar"
        if full.is_file():
            v[f"imagefull_{a}"], v[f"imagefull_digest_{a}"] = sha256(full), image_digest(full)
    return pins, v


def manifest(images: pathlib.Path, release: str, assets: pathlib.Path) -> str:
    """The manifest of a release, as JSON (T-499, githome's G-23)."""
    pins, v = values(images, assets)

    def image(kind: str, a: str) -> dict | None:
        name = "runner" if kind == "image" else "runner-full"
        if not v[f"{kind}_{a}"]:
            return None
        return {"file": f"{name}-{a}.tar", "sha256": v[f"{kind}_{a}"], "digest": v[f"{kind}_digest_{a}"]}

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
