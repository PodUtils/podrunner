#!/bin/sh
# SPDX-License-Identifier: 0BSD
# Pack the candidate binary and its manual for an outside test.
set -eu
umask 022

if [ "$#" -ne 2 ]; then
    echo "usage: sh scripts/qa-bundle.sh BINARY OUTPUT_DIR" >&2
    exit 2
fi
if [ ! -f "$1" ] || [ ! -x "$1" ]; then
    printf 'qa-bundle: %s is not an executable file\n' "$1" >&2
    exit 2
fi

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)
parent=$(CDPATH='' cd -- "$(dirname -- "$2")" && pwd -P)
output=$parent/$(basename -- "$2")
if [ -e "$output" ] || [ -L "$output" ]; then
    printf 'qa-bundle: %s already exists\n' "$output" >&2
    exit 2
fi

stage=$(mktemp -d -- "$parent/.podrunner-qa.XXXXXXXX")
trap 'rm -rf -- "$stage"' EXIT
trap 'exit 1' HUP INT TERM
install -m 0755 -- "$1" "$stage/podrunner"
if ! TERM=dumb timeout --kill-after=5 30 "$stage/podrunner" man --roff </dev/null >"$stage/podrunner.1" ||
   [ ! -s "$stage/podrunner.1" ]; then
    echo "qa-bundle: the candidate did not produce a manual" >&2
    exit 1
fi
install -m 0644 -- "$root/LICENSE" "$root/THIRD_PARTY.md" "$stage"
(
    cd "$stage"
    sha256sum podrunner podrunner.1 LICENSE THIRD_PARTY.md >SHA256SUMS
)
chmod 0755 -- "$stage"
mv -nT -- "$stage" "$output"
if [ -d "$stage" ]; then
    printf 'qa-bundle: %s already exists\n' "$output" >&2
    exit 2
fi
printf 'qa-bundle: %s\n' "$output"
