#!/bin/sh
# SPDX-License-Identifier: 0BSD
# The test of githome's source (T-509, githome's G-38): the pinned
# pod-fake-githome of podcrates (images/fake-githome.toml) against each test
# of `podrunner source-conformance`. A failed test fails the gate, and a red
# gate blocks release.yml.
#
#   sh scripts/githome-conformance.sh PODRUNNER [DIR]
#
# DIR keeps the download (default: a new temporary directory).
set -eu
[ $# -ge 1 ] || { echo "usage: sh scripts/githome-conformance.sh PODRUNNER [DIR]" >&2; exit 64; }
podrunner=$1
root=$(cd "$(dirname "$0")/.." && pwd)
dir=${2:-$(mktemp -d)}
mkdir -p "$dir"
case $(uname -m) in
    x86_64 | amd64) arch=x86_64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *) echo "githome-conformance: no asset for $(uname -m)" >&2; exit 1 ;;
esac
pins=$root/images/fake-githome.toml
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$pins")
sum=$(sed -n "s/^$arch = \"\\([0-9a-f]*\\)\"\$/\\1/p" "$pins")
[ -n "$version" ] && [ -n "$sum" ] || { echo "githome-conformance: $pins has no version or no sum of $arch" >&2; exit 1; }
bin=$dir/pod-fake-githome
if ! printf '%s  %s\n' "$sum" "$bin" | sha256sum -c --status 2> /dev/null; then
    url=https://github.com/PodUtils/podcrates/releases/download/v$version/pod-fake-githome-$arch-unknown-linux-musl
    sh "$root/scripts/retry.sh" curl -qfsSL -o "$bin.part" "$url"
    printf '%s  %s\n' "$sum" "$bin.part" | sha256sum -c --status ||
        { echo "githome-conformance: the SHA-256 of $url is not the pin" >&2; rm -f "$bin.part"; exit 1; }
    mv -f "$bin.part" "$bin"
fi
chmod 0755 "$bin"
"$bin" serve > "$dir/serve.out" 2> "$dir/serve.err" &
serve=$!
trap 'kill "$serve" 2> /dev/null || true' EXIT INT TERM
i=0
until [ -s "$dir/serve.out" ]; do
    i=$((i + 1))
    [ "$i" -le 50 ] || { echo "githome-conformance: serve gave no URL" >&2; cat "$dir/serve.err" >&2; exit 1; }
    sleep 0.1
done
base=$(head -n 1 "$dir/serve.out")
base=${base%/}
# One queued job for each test that takes one, and some more.
tests=$("$podrunner" source-conformance --json -- /bin/false 2> /dev/null | head -n 1 |
    sed -n 's/.*"tests":\([0-9]*\).*/\1/p')
[ -n "$tests" ] || { echo "githome-conformance: podrunner names no count of tests" >&2; exit 1; }
n=0
while [ "$n" -lt $((tests + 2)) ]; do
    curl -qfsS -o /dev/null -X POST "$base/_fake/jobs" -H 'Content-Type: application/json' \
        -d '{"repository":"example-org/app","tier":"trusted"}'
    n=$((n + 1))
done
out=$dir/conformance.jsonl
POD_FAKE_GITHOME=$base "$podrunner" source-conformance --json -- "$bin" source > "$out" || true
last=$(tail -n 1 "$out")
case $last in
    *"\"failed\":0"*) echo "githome-conformance: pod-fake-githome $version passes the $tests tests: $last" ;;
    *)
        echo "githome-conformance: pod-fake-githome $version fails: $last" >&2
        grep '"pass":false' "$out" >&2 || true
        exit 1
        ;;
esac
