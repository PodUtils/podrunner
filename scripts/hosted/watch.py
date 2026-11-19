#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The watcher of the hosted images (T-540).

    python3 -I scripts/hosted/watch.py --labels images/hosted/labels.toml \\
        --releases releases.json [--summary FILE] IMAGE_FILE...
    python3 -I scripts/hosted/watch.py --self-test

Each IMAGE_FILE is written by a job on one label: "label<TAB>ImageOS<TAB>
ImageVersion<TAB>arch". The label of a job is the truth: the README of
runner-images can lag a move of `ubuntu-latest`. Each version is matched
to its release of runner-images by the build number (`20261111.370.1` is
the release `ubuntu24/*.370`): the date of a tag can differ from the date
of the version. It fails when a version has no release, or when a label
runs another OS than images/hosted/labels.toml says (a moved label). A
version that is newer than the pin of its label is reported, for a capture.
"""

import argparse
import json
import pathlib
import sys
import tomllib


def release_of(version: str, image_os: str, arch: str, releases: list[dict]) -> dict | None:
    parts = version.split(".")
    if len(parts) < 2 or not parts[1].isdigit():
        return None
    prefix = image_os + ("-arm64" if arch in ("arm64", "aarch64", "ARM64") else "") + "/"
    hits = [r for r in releases if r["tag_name"].startswith(prefix) and r["tag_name"].endswith("." + parts[1])]
    return hits[0] if len(hits) == 1 else None


def judge(labels: dict, images: list[list[str]], releases: list[dict]) -> tuple[list[str], list[str]]:
    """(the rows of the report, the failures)."""
    rows, bad = [], []
    for label, image_os, version, arch in images:
        want = labels.get(label)
        if want is None:
            bad.append(f"{label}: not in labels.toml")
            continue
        if want["image_os"] != image_os:
            bad.append(f"{label}: runs {image_os}, labels.toml says {want['image_os']}: the label moved")
        r = release_of(version, image_os, arch, releases)
        if r is None:
            bad.append(f"{label}: no release of runner-images for {image_os} {version} ({arch})")
            continue
        tag, commit = r["tag_name"], r["target_commitish"]
        pin = want.get("release", "")
        state = "pinned" if tag == pin else ("new: capture it" if pin else "no pin")
        rows.append(f"| `{label}` | {image_os} | {version} | `{tag}` | `{commit[:12]}` | {state} |")
    return rows, bad


def self_test() -> int:
    releases = [
        {"tag_name": "ubuntu24/20261023.334", "target_commitish": "a" * 40},
        {"tag_name": "ubuntu24/20261111.370", "target_commitish": "b" * 40},
        {"tag_name": "ubuntu24-arm64/20261111.182", "target_commitish": "c" * 40},
        {"tag_name": "ubuntu26/20261111.196", "target_commitish": "d" * 40},
    ]
    labels = {
        "ubuntu-24.04": {"image_os": "ubuntu24", "release": "ubuntu24/20261023.334"},
        "ubuntu-24.04-arm": {"image_os": "ubuntu24", "release": ""},
        "ubuntu-latest": {"image_os": "ubuntu26", "release": "ubuntu26/20261111.196"},
    }
    bad = []
    # The date of the tag differs from the date of the version: the build
    # number matches.
    r = release_of("20261012.334.1", "ubuntu24", "X64", releases)
    if not r or r["tag_name"] != "ubuntu24/20261023.334":
        bad.append(f"the build number: {r}")
    if release_of("20261111.182.1", "ubuntu24", "ARM64", releases)["tag_name"] != "ubuntu24-arm64/20261111.182":
        bad.append("arm64")
    rows, b = judge(labels, [["ubuntu-24.04", "ubuntu24", "20261111.370.1", "X64"]], releases)
    if b or "new: capture it" not in rows[0]:
        bad.append(f"a newer version: {rows} {b}")
    rows, b = judge(labels, [["ubuntu-latest", "ubuntu24", "20261111.370.1", "X64"]], releases)
    if not any("the label moved" in x for x in b):
        bad.append(f"a moved label: {b}")
    rows, b = judge(labels, [["ubuntu-24.04", "ubuntu24", "20261201.999.1", "X64"]], releases)
    if not any("no release" in x for x in b):
        bad.append(f"no release: {b}")
    for x in bad:
        print(f"self-test: {x}")
    if not bad:
        print("watch.py: self-test passes")
    return 1 if bad else 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    ap = argparse.ArgumentParser()
    ap.add_argument("--labels", type=pathlib.Path, required=True)
    ap.add_argument("--releases", type=pathlib.Path, required=True)
    ap.add_argument("--summary", type=pathlib.Path)
    ap.add_argument("images", nargs="+", type=pathlib.Path)
    a = ap.parse_args()
    labels = {x["name"]: x for x in tomllib.loads(a.labels.read_text())["label"]}
    images = [p.read_text().strip("\n").split("\t") for p in a.images]
    rows, bad = judge(labels, images, json.loads(a.releases.read_text()))
    text = "| Label | ImageOS | ImageVersion | Release | Commit | State |\n| --- | --- | --- | --- | --- | --- |\n"
    text += "\n".join(rows) + "\n" + "".join(f"\n{b}" for b in bad) + "\n"
    print(text)
    if a.summary:
        with a.summary.open("a") as f:
            f.write("## The hosted images\n\n" + text)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
