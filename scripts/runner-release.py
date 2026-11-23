#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The runner image release (T-499 step 1): a release of the runner images
of a new actions/runner, with no new podrunner binary.
.github/workflows/runner-image-release.yml calls each command.

  runner-release.py newest RELEASES
      print the newest release of actions/runner that is not a draft and
      not a prerelease
  runner-release.py pin VERSION RELEASE OUT
      check the release VERSION of actions/runner, and write OUT in the
      form of images/runner/runner.toml; print version=, sha256_x86_64=,
      sha256_aarch64= and released_at_s= lines
  runner-release.py tag VERSION REVISION
      print the tag of the image release: vrunner-VERSION-rREVISION
  runner-release.py detect RELEASES OWN RUNNER_TOML [--now SECONDS]
      exit 1 when the newest actions/runner has no runner image: not the
      pin of RUNNER_TOML and no image release in OWN
  runner-release.py manifest --tag TAG --images DIR --assets DIR
          --commit SHA --released-at SECONDS [--audit TEXT] --out FILE
      write runner-images.json, the manifest that a machine reads; TEXT is
      the result of the audit of the NuGet advisories of the runner
  runner-release.py notes MANIFEST OUT [--run URL]
      write the notes of the image release
  runner-release.py --self-test

RELEASES and OWN are answers of GitHub's REST API: GET
/repos/actions/runner/releases and GET /repos/PodUtils/podrunner/releases,
as a JSON list or as one JSON object on each line. RELEASE is GET
/repos/actions/runner/releases/tags/vVERSION.
"""

import argparse
import importlib.util
import json
import pathlib
import re
import sys
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
ARCHES = {"x86_64": "linux-x64", "aarch64": "linux-arm64"}
# Each check is a full match: `$` of `re.match` takes a trailing newline.
VERSION = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")
TAG = re.compile(r"vrunner-((?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*))-r([1-9][0-9]*)")
# The days from a release of actions/runner to its runner image (G-23).
TARGET_DAYS = 14
DAY = 24 * 3600


class Fault(Exception):
    pass


def pins_module():
    """scripts/install-pins.py: the digest, the size and the time of the
    build of an image archive, as the manifest of a podrunner release
    measures them."""
    spec = importlib.util.spec_from_file_location("install_pins", ROOT / "scripts" / "install-pins.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def releases(text: str) -> list[dict]:
    text = text.strip()
    if not text:
        return []
    if text.startswith("["):
        return json.loads(text)
    return [json.loads(line) for line in text.splitlines() if line.strip()]


def key(version: str) -> tuple[int, ...]:
    return tuple(int(p) for p in version.split("."))


def newest(rels: list[dict]) -> dict:
    stable = [r for r in rels if not r.get("draft") and not r.get("prerelease")
              and VERSION.fullmatch(r.get("tag_name", "").removeprefix("v"))]
    if not stable:
        raise Fault("no release of actions/runner that is not a draft or a prerelease")
    return max(stable, key=lambda r: key(r["tag_name"][1:]))


def epoch(iso: str) -> int:
    import datetime

    return int(datetime.datetime.fromisoformat(iso.replace("Z", "+00:00")).timestamp())


def tarball_sums(version: str, rel: dict) -> dict[str, str]:
    """The SHA-256 of each Linux tarball: the one of the release notes. When
    the API names the digest of the asset too, both must agree."""
    if not VERSION.fullmatch(version):
        raise Fault(f"{version!r} is not a version X.Y.Z")
    if rel.get("tag_name") != f"v{version}":
        raise Fault(f"the release is {rel.get('tag_name')!r}, not v{version}")
    if rel.get("draft"):
        raise Fault(f"v{version} is a draft")
    if rel.get("prerelease"):
        raise Fault(f"v{version} is a prerelease: GitHub does not ask a runner to take it")
    body = rel.get("body") or ""
    assets = {a.get("name"): a for a in rel.get("assets", [])}
    out = {}
    for arch, name in ARCHES.items():
        found = re.findall(rf"<!-- BEGIN SHA {name} -->\s*([0-9a-f]{{64}})\s*<!-- END SHA {name} -->", body)
        if len(found) != 1:
            raise Fault(f"v{version}: {len(found)} SHA-256 of {name} in the release notes, not one")
        file = f"actions-runner-{name}-{version}.tar.gz"
        if file not in assets:
            raise Fault(f"v{version}: no asset {file}")
        digest = assets[file].get("digest")
        if digest and digest != f"sha256:{found[0]}":
            raise Fault(f"v{version}: {file}: the notes give {found[0]}, the API gives {digest}")
        out[arch] = found[0]
    return out


def runner_toml(version: str, sums: dict[str, str]) -> str:
    return (
        "# The official runner of the unit image (GitHub #30). A change of this file\n"
        "# is a reviewed commit: `podrunner image verify` and the Dockerfile check\n"
        "# the release tarball against these digests. The digests are the ones of\n"
        "# the release notes of actions/runner.\n"
        f'version = "{version}"\n\n[sha256]\n'
        + "".join(f'{a} = "{sums[a]}"\n' for a in ARCHES)
    )


def tag(version: str, revision: str) -> str:
    t = f"vrunner-{version}-r{revision}"
    if not TAG.fullmatch(t):
        raise Fault(f"{t!r} is not vrunner-X.Y.Z-rN")
    return t


def detect(rels: list[dict], own: list[dict], pinned: str, now: int) -> tuple[int, list[str]]:
    r = newest(rels)
    version = r["tag_name"][1:]
    age = (now - epoch(r["published_at"])) // DAY if r.get("published_at") else None
    images = []
    for o in own:
        m = TAG.fullmatch(o.get("tag_name", ""))
        if m and m[1] == version and not o.get("draft"):
            images.append(m[0])
    images.sort()
    lines = [f"the newest actions/runner: {version}" + (f" (published {age} days ago)" if age is not None else "")]
    lines.append(f"the pin of images/runner/runner.toml: {pinned}")
    lines.append(f"the image releases of {version}: {', '.join(images) or 'none'}")
    if pinned == version or images:
        if pinned != version:
            lines.append(f"pin {version} in images/runner/runner.toml for the next podrunner release")
        return 0, lines
    left = TARGET_DAYS - age if age is not None else TARGET_DAYS
    lines.append(f"no runner image of {version}: dispatch runner-image-release.yml with runner_version={version} "
                 f"({left} days left of the {TARGET_DAYS} days of T-499)")
    return 1, lines


def manifest(tag_name: str, images: pathlib.Path, assets: pathlib.Path, commit: str, released_at: int,
             audit: str = "") -> dict:
    if not TAG.fullmatch(tag_name):
        raise Fault(f"{tag_name!r} is not vrunner-X.Y.Z-rN")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise Fault(f"--commit {commit!r}: give 40 lowercase hex digits")
    p = pins_module()
    version, sums = p.toml_pin(images / "runner" / "runner.toml")
    if version != TAG.fullmatch(tag_name)[1]:
        raise Fault(f"{tag_name}: the runner of {images}/runner/runner.toml is {version}")

    def image(name: str) -> dict:
        f = assets / name
        if not f.is_file():
            raise Fault(f"{f}: missing: the image release needs it")
        return {"file": name, "sha256": p.sha256(f), "digest": p.image_digest(f),
                "built_at_s": p.image_created(f), "unpacked_bytes": p.image_unpacked(f)}

    return {
        "schema": 1,
        "kind": "runner-images",
        "tag": tag_name,
        "commit": commit,
        # The same form as "runner" of podrunner-manifest.json.
        "runner": {
            "version": version,
            "released_at_s": released_at,
            "tarball_sha256": sums,
            "images": {a: {"base": image(f"runner-{a}.tar"), "full": image(f"runner-full-{a}.tar")} for a in ARCHES},
        },
        # The podvm and the podnet that the proof of the images ran.
        "podvm": dict(zip(("version", "sha256"), p.toml_pin(images / "podvm.toml"))),
        "podnet": dict(zip(("version", "sha256"), p.toml_pin(images / "podnet.toml"))),
        # The NuGet advisories of the runner against images/runner/advisories.txt.
        "advisories": audit or "not run",
    }


def notes(doc: dict, run: str) -> str:
    r = doc["runner"]
    t = doc["tag"]
    out = [
        f"The runner images of actions/runner {r['version']}, with no new podrunner binary (T-499).",
        "",
        f"Commit `{doc['commit']}`. podvm `{doc['podvm']['version']}`, podnet `{doc['podnet']['version']}`.",
        "",
        "| Architecture | Image | File | Manifest digest | Unpacked |",
        "| --- | --- | --- | --- | --- |",
    ]
    for a, kinds in r["images"].items():
        for kind, i in kinds.items():
            out.append(f"| {a} | {kind} | `{i['file']}` | `{i['digest']}` | {i['unpacked_bytes'] >> 20} MiB |")
    out += [
        "",
        "## Check",
        "",
        "```sh",
        "sha256sum -c --ignore-missing SHA256SUMS",
        f"pod-release verify --repo PodUtils/podrunner --tag {t} --workflow runner-image-release.yml DIR",
        "```",
        "",
        "## Load",
        "",
        "```sh",
        "PODVM_STORE=~/.local/share/podrunner/images podvm load -i runner-ARCH.tar",
        "```",
        "",
        "Then set `image = \"runner@DIGEST\"` in `run.toml`, or add the pin below to `podrunner.toml`",
        "and sign the pin list again (`podrunner pin-sign`). `docs/install.md`, \"A newer runner image\".",
        "",
        "```toml",
    ]
    for a, kinds in r["images"].items():
        i = kinds["base"]
        out += ["[[pin.payload]]", f'arch = "{a}"', f'reference = "runner@{i["digest"]}"',
                f"built_at_s = {i['built_at_s']}", f'runner_version = "{r["version"]}"',
                f"unpacked_bytes = {i['unpacked_bytes']}", ""]
    out[-1] = "```"
    out += [
        "",
        "## The level of proof",
        "",
        f"- Proven in CI on GitHub-hosted runners ({run or 'the run of runner-image-release.yml'}): each image",
        "  loads with `podvm load -i` and holds the runner files and the runner version; on x86_64 the",
        "  official runner in a podvm unit runs the canned job of the fake broker, `podrunner run` runs",
        "  the job of the hosted work folder, and the larger image runs a job of `runs-on: ubuntu-latest`.",
        "- Not run: a constrained host as a user; a job of github.com.",
        f"- The NuGet advisories of the runner against `images/runner/advisories.txt`: {doc['advisories']}.",
        "",
    ]
    return "\n".join(out)


def self_test() -> int:
    import io
    import tarfile

    faults: list[str] = []

    def check(name: str, cond: bool, why: str = "") -> None:
        if not cond:
            faults.append(f"{name}: {why}")
        print(f"{'ok' if cond else 'FAULT'} {name}")

    def refused(name: str, fn, want: str) -> None:
        try:
            fn()
        except Fault as e:
            check(name, want in str(e), f"the fault {e} does not name {want!r}")
            return
        check(name, False, "no fault")

    a, b = "a" * 64, "b" * 64
    body = f"x\n- <!-- BEGIN SHA linux-x64 -->{a}<!-- END SHA linux-x64 -->\n- <!-- BEGIN SHA linux-arm64 -->{b}<!-- END SHA linux-arm64 -->\n"

    def rel(v: str, **kw) -> dict:
        r = {"tag_name": f"v{v}", "draft": False, "prerelease": False, "published_at": "2026-11-01T00:00:00Z", "body": body,
             "assets": [{"name": f"actions-runner-linux-x64-{v}.tar.gz", "digest": f"sha256:{a}"},
                        {"name": f"actions-runner-linux-arm64-{v}.tar.gz"}]}
        r.update(kw)
        return r

    rels = [rel("2.338.2"), rel("2.339.0", prerelease=True), rel("2.338.10"), rel("2.340.0", draft=True)]
    check("newest", newest(rels)["tag_name"] == "v2.338.10", "not v2.338.10")
    check("a JSON line on each line", len(releases("\n".join(json.dumps(r) for r in rels))) == 4)
    check("the sums of the notes", tarball_sums("2.338.2", rel("2.338.2")) == {"x86_64": a, "aarch64": b})
    toml = tomllib.loads(runner_toml("2.338.2", {"x86_64": a, "aarch64": b}))
    check("runner.toml", toml == {"version": "2.338.2", "sha256": {"x86_64": a, "aarch64": b}}, str(toml))
    refused("a prerelease", lambda: tarball_sums("2.339.0", rel("2.339.0", prerelease=True)), "prerelease")
    refused("another tag", lambda: tarball_sums("2.338.2", rel("2.338.3")), "not v2.338.2")
    refused("no sum of arm64", lambda: tarball_sums("2.338.2", rel("2.338.2", body=body.split("\n- <!-- BEGIN SHA linux-arm64")[0])),
            "0 SHA-256 of linux-arm64")
    refused("two sums of x64", lambda: tarball_sums("2.338.2", rel("2.338.2", body=body + body)), "2 SHA-256 of linux-x64")
    bad = rel("2.338.2")
    bad["assets"][0]["digest"] = f"sha256:{b}"
    refused("the API and the notes differ", lambda: tarball_sums("2.338.2", bad), "the API gives")
    refused("no tarball", lambda: tarball_sums("2.338.2", rel("2.338.2", assets=[])), "no asset")
    refused("not a version", lambda: tarball_sums("2.338", rel("2.338")), "not a version")
    check("tag", tag("2.339.0", "1") == "vrunner-2.339.0-r1")
    refused("a bad revision", lambda: tag("2.339.0", "0"), "not vrunner")
    refused("a bad version", lambda: tag("2.339", "1"), "not vrunner")
    # `$` of `re.match` takes a trailing newline: each check is a full match.
    refused("a version with a newline", lambda: tag("2.339.0\n", "1"), "not vrunner")
    refused("a revision with a newline", lambda: tag("2.339.0", "1\n"), "not vrunner")
    refused("a version of the notes with a newline", lambda: tarball_sums("2.338.2\n", rel("2.338.2\n")), "not a version")
    check("newest: a tag with a newline is no release", newest(rels + [rel("2.341.0\n")])["tag_name"] == "v2.338.10")

    now = epoch("2026-11-23T00:00:00Z")
    rc, lines = detect([rel("2.338.2")], [], "2.338.2", now)
    check("detect: the pin is the newest", rc == 0, "\n".join(lines))
    rc, lines = detect([rel("2.338.2"), rel("2.339.0", published_at="2026-11-20T00:00:00Z")], [], "2.338.2", now)
    check("detect: a new runner with no image", rc == 1 and "11 days left" in lines[-1], "\n".join(lines))
    own = [{"tag_name": "vrunner-2.339.0-r1", "draft": False}, {"tag_name": "v0.1.0-beta.7", "draft": False}]
    rc, lines = detect([rel("2.338.2"), rel("2.339.0")], own, "2.338.2", now)
    check("detect: an image release", rc == 0 and "vrunner-2.339.0-r1" in lines[2], "\n".join(lines))
    rc, _ = detect([rel("2.339.0")], [{"tag_name": "vrunner-2.339.0-r1", "draft": True}], "2.338.2", now)
    check("detect: a draft is no image release", rc == 1)
    rc, _ = detect([rel("2.339.0")], [{"tag_name": "vrunner-2.339.0-r1\n", "draft": False}], "2.338.2", now)
    check("detect: a tag with a newline is no image release", rc == 1)

    with tempfile.TemporaryDirectory() as d:
        d = pathlib.Path(d)
        (d / "images" / "runner").mkdir(parents=True)
        (d / "images" / "runner" / "runner.toml").write_text(runner_toml("2.339.0", {"x86_64": a, "aarch64": b}))
        for n in ("podvm", "podnet"):
            (d / "images" / f"{n}.toml").write_text(f'version = "1.0.0"\n[sha256]\nx86_64 = "{a}"\naarch64 = "{b}"\n')

        def archive(path: pathlib.Path, ref: str) -> None:
            def blob(data: bytes) -> tuple[str, bytes]:
                import hashlib

                return "sha256:" + hashlib.sha256(data).hexdigest(), data

            layer_buf = io.BytesIO()
            with tarfile.open(fileobj=layer_buf, mode="w") as lt:
                info = tarfile.TarInfo("home/runner/run.sh")
                info.size = 5
                lt.addfile(info, io.BytesIO(b"echo\n"))
            layer = blob(layer_buf.getvalue())
            config = blob(b'{"created": "2026-11-20T00:00:00Z"}')
            man = blob(json.dumps({"config": {"digest": config[0]}, "layers": [
                {"mediaType": "application/vnd.oci.image.layer.v1.tar", "digest": layer[0]}]}).encode())
            index = json.dumps({"schemaVersion": 2, "manifests": [
                {"digest": man[0], "annotations": {"org.opencontainers.image.ref.name": ref}}]}).encode()
            with tarfile.open(path, "w") as t:
                for name, data in [("index.json", index)] + [(f"blobs/sha256/{x[0][7:]}", x[1]) for x in (layer, config, man)]:
                    info = tarfile.TarInfo(name)
                    info.size = len(data)
                    t.addfile(info, io.BytesIO(data))

        for arch in ARCHES:
            archive(d / f"runner-{arch}.tar", "runner:2.339.0")
            archive(d / f"runner-full-{arch}.tar", "runner:full")
        doc = manifest("vrunner-2.339.0-r1", d / "images", d, "c" * 40, 1763337600)
        base = doc["runner"]["images"]["aarch64"]["base"]
        check("manifest", doc["runner"]["version"] == "2.339.0" and base["unpacked_bytes"] == 5
              and base["built_at_s"] == epoch("2026-11-20T00:00:00Z") and base["digest"].startswith("sha256:")
              and doc["podvm"]["version"] == "1.0.0" and doc["advisories"] == "not run", json.dumps(doc))
        doc["advisories"] = "the same as the reviewed list"
        text = notes(doc, "")
        check("notes", f'reference = "runner@{base["digest"]}"' in text and "--workflow runner-image-release.yml" in text
              and "unpacked_bytes = 5" in text and text.count("[[pin.payload]]") == 2
              and "advisories.txt`: the same as the reviewed list" in text, text)
        refused("manifest: another runner", lambda: manifest("vrunner-2.338.2-r1", d / "images", d, "c" * 40, 0), "2.339.0")
        (d / "runner-full-aarch64.tar").unlink()
        refused("manifest: no full image", lambda: manifest("vrunner-2.339.0-r1", d / "images", d, "c" * 40, 0), "missing")
    for f in faults:
        print(f, file=sys.stderr)
    print(f"runner-release: self-test {'failed' if faults else 'passed'}")
    return 1 if faults else 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    p = argparse.ArgumentParser(prog="runner-release.py")
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("newest").add_argument("releases")
    s = sub.add_parser("pin")
    s.add_argument("version")
    s.add_argument("release")
    s.add_argument("out")
    s = sub.add_parser("tag")
    s.add_argument("version")
    s.add_argument("revision")
    s = sub.add_parser("detect")
    s.add_argument("releases")
    s.add_argument("own")
    s.add_argument("runner_toml")
    s.add_argument("--now", type=int)
    s = sub.add_parser("manifest")
    for k in ("--tag", "--images", "--assets", "--commit", "--out"):
        s.add_argument(k, required=True)
    s.add_argument("--released-at", type=int, required=True)
    s.add_argument("--audit", default="")
    s = sub.add_parser("notes")
    s.add_argument("manifest")
    s.add_argument("out")
    s.add_argument("--run", default="")
    o = p.parse_args()
    read = lambda f: pathlib.Path(f).read_text()  # noqa: E731
    try:
        if o.cmd == "newest":
            print(newest(releases(read(o.releases)))["tag_name"][1:])
        elif o.cmd == "pin":
            rel = json.loads(read(o.release))
            sums = tarball_sums(o.version, rel)
            pathlib.Path(o.out).write_text(runner_toml(o.version, sums))
            print(f"version={o.version}")
            for arch, s in sums.items():
                print(f"sha256_{arch}={s}")
            print(f"released_at_s={epoch(rel['published_at']) if rel.get('published_at') else 0}")
        elif o.cmd == "tag":
            print(tag(o.version, o.revision))
        elif o.cmd == "detect":
            import time

            pinned = tomllib.loads(read(o.runner_toml))["version"]
            rc, lines = detect(releases(read(o.releases)), releases(read(o.own)), pinned, o.now or int(time.time()))
            print("\n".join(lines))
            return rc
        elif o.cmd == "manifest":
            doc = manifest(o.tag, pathlib.Path(o.images), pathlib.Path(o.assets), o.commit, o.released_at, o.audit)
            pathlib.Path(o.out).write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
            print(f"{o.out}: the manifest of {o.tag}")
        elif o.cmd == "notes":
            pathlib.Path(o.out).write_text(notes(json.loads(read(o.manifest)), o.run))
    except Fault as e:
        print(f"runner-release: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
