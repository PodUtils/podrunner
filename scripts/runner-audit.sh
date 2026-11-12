#!/bin/sh
# Compare the NuGet advisories of the official runner with the reviewed list
# (T-082). The runner payload of each unit image is the release of
# images/runner/runner.toml; its packages get advisories after the release.
#
#   sh scripts/runner-audit.sh SRC LIST
#
# SRC is a clone of actions/runner at the tag of images/runner/runner.toml,
# with the .NET SDK of its src/global.json on PATH. LIST is
# images/runner/advisories.txt: one line "PACKAGE VERSION GHSA-ID" for each
# reviewed advisory. The script prints each advisory of a shipped project
# (not Test) and exits 1 when the list misses one or names one that is
# gone. It does not see the advisories of the .NET runtime itself.
set -eu
[ $# -eq 2 ] || {
    echo "usage: sh scripts/runner-audit.sh SRC LIST" >&2
    exit 2
}
src=$1
list=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
out=$(mktemp)
trap 'rm -f "$out"' EXIT
cd "$src/src"
export DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1
# The audit of the restore fails on an advisory: the list below reads it.
dotnet restore ActionsRunner.sln -p:NuGetAudit=false -v q > /dev/null
# dotnet list exits 1 when it finds an advisory; the JSON tells.
dotnet list ActionsRunner.sln package --vulnerable --include-transitive --format json > "$out" || true
python3 -I - "$out" "$list" <<'PY'
import json, sys

data = json.load(open(sys.argv[1]))
if "projects" not in data:
    sys.exit("runner-audit: no projects in the output of dotnet list")
found = set()
for project in data["projects"]:
    if project["path"].replace("\\", "/").endswith("/Test.csproj"):
        continue
    for framework in project.get("frameworks", []):
        for kind in ("topLevelPackages", "transitivePackages"):
            for package in framework.get(kind, []):
                for v in package.get("vulnerabilities", []):
                    ghsa = v["advisoryurl"].rstrip("/").rsplit("/", 1)[-1]
                    found.add(f"{package['id']} {package['resolvedVersion']} {ghsa}")
listed = set()
for line in open(sys.argv[2]):
    line = line.split("#", 1)[0].strip()
    if line:
        listed.add(" ".join(line.split()))
for a in sorted(found):
    print(f"runner-audit: {a}" + ("" if a in listed else " (not in the list)"))
for a in sorted(listed - found):
    print(f"runner-audit: {a} (in the list, gone from the runner)")
if found != listed:
    sys.exit("runner-audit: the list and the advisories of the runner differ")
print(f"runner-audit: {len(found)} advisories, each one reviewed")
PY
