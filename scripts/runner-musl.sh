#!/bin/sh
# Build the layout of the official runner for a musl runtime id (T-081,
# P0-4 of GitHub #67). actions/runner knows no musl id: this script patches
# a source tree of a release tag, then runs its dev.sh layout. A patch that
# finds no match stops the script: a new runner release must not build a
# glibc layout under a musl name.
#
#   sh scripts/runner-musl.sh SRC RID
#
# SRC is a clone of actions/runner at the tag of images/runner/runner.toml.
# RID is linux-musl-x64 or linux-musl-arm64. The layout is SRC/_layout.
# Run it on a glibc host: the .NET SDK of dev.sh is glibc.
set -eu
[ $# -eq 2 ] || {
    echo "usage: sh scripts/runner-musl.sh SRC RID" >&2
    exit 2
}
src=$1
rid=$2
case $rid in
linux-musl-x64 | linux-musl-arm64) ;;
*)
    echo "runner-musl: unknown runtime id: $rid" >&2
    exit 2
    ;;
esac
python3 -I - "$src/src" "$rid" <<'PY'
import pathlib, sys

src, rid = pathlib.Path(sys.argv[1]), sys.argv[2]
glibc = rid.replace("musl-", "")
define = {"linux-musl-x64": "X64", "linux-musl-arm64": "ARM64"}[rid]


def patch(path, old, new, count=1):
    text = path.read_text()
    found = text.count(old)
    if found != count:
        sys.exit(f"runner-musl: {path.name}: {found} matches of {old!r}, not {count}")
    path.write_text(text.replace(old, new))


# dev.sh refuses each Linux id but the four glibc ones.
patch(src / "dev.sh", "(\"$RUNTIME_ID\" != 'linux-arm') ]]",
      f"(\"$RUNTIME_ID\" != 'linux-arm') && (\"$RUNTIME_ID\" != '{rid}') ]]")
# The define of the architecture comes from the runtime id.
patch(src / "Directory.Build.props", f"'$(PackageRuntime)' == '{glibc}'",
      f"('$(PackageRuntime)' == '{glibc}' OR '$(PackageRuntime)' == '{rid}')")
# The audit fails a tag on each advisory newer than the tag (warnings are
# errors). This layout is a measure and ships nothing: no audit.
patch(src / "Directory.Build.props", "<NuGetAudit>true</NuGetAudit>", "<NuGetAudit>false</NuGetAudit>")
# A restore makes assets only for the listed ids; a publish needs its id.
ids = "win-x64;win-x86;linux-x64;linux-arm64;linux-arm;osx-x64;osx-arm64;win-arm64"
projects = sorted(src.glob("*/*.csproj"))
if len(projects) < 7:
    sys.exit(f"runner-musl: {len(projects)} projects, not 7 or more")
for p in projects:
    patch(p, ids, f"{ids};{rid}")
# Node of the layout: on x64 the musl builds of actions/alpine_nodejs, as
# the glibc layout has them (node20_alpine); on arm64, which alpine_nodejs
# does not build, the musl builds of unofficial-builds.nodejs.org.
if rid == "linux-musl-x64":
    get = ['"$NODE_ALPINE_URL/v${V}/node-v${V}-alpine-x64.tar.gz" N']
else:
    get = ['"https://unofficial-builds.nodejs.org/download/release/v${V}/node-v${V}-linux-arm64-musl.tar.gz" N fix_nested_dir']
lines = "".join(
    "    acquireExternalTool " + get[0].replace("${V}", "${NODE%s_VERSION}" % n).replace(" N", " node" + n) + "\n"
    for n in ("20", "24")
)
patch(src / "Misc/externals.sh", "if [[ \"$PACKAGERUNTIME\" == \"linux-arm64\" ]]; then",
      f"if [[ \"$PACKAGERUNTIME\" == \"{rid}\" ]]; then\n{lines}fi\n\n"
      "if [[ \"$PACKAGERUNTIME\" == \"linux-arm64\" ]]; then")
print(f"runner-musl: patched {len(projects)} projects for {rid}")
PY
cd "$src/src"
./dev.sh layout Release "$rid"
# A musl layout links the musl loader, not the glibc one.
interp=$(readelf -l ../_layout/bin/Runner.Listener | sed -n 's/.*interpreter: \(.*\)]/\1/p')
case $interp in
/lib/ld-musl-*) echo "runner-musl: Runner.Listener uses $interp" ;;
*)
    echo "runner-musl: Runner.Listener uses ${interp:-no interpreter}, not musl" >&2
    exit 1
    ;;
esac
