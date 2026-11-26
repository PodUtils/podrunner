#!/bin/sh
# SPDX-License-Identifier: 0BSD
# One pinned version of actionlint, the same in CI and on a host of an agent
# (T-252). It uses the pinned ShellCheck of scripts/shellcheck.sh on x86_64.
# The binary goes into .work/tools, checked by its SHA-256.
#
#   sh scripts/actionlint.sh [ARGUMENT...]
set -eu
version=1.7.7
root=$(cd "$(dirname "$0")/.." && pwd)
case "$(uname -m)" in
x86_64)
    arch=amd64
    sum=023070a287cd8cccd71515fedc843f1985bf96c436b7effaecce67290e7e0757
    ;;
aarch64 | arm64)
    arch=arm64
    sum=401942f9c24ed71e4fe71b76c7d638f66d8633575c4016efd2977ce7c28317d0
    ;;
*)
    echo "actionlint.sh: no pin for $(uname -m); install actionlint $version" >&2
    exit 69
    ;;
esac
dir=$root/.work/tools/actionlint-v$version-$arch
bin=$dir/actionlint
if [ ! -x "$bin" ]; then
    mkdir -p "$dir"
    tmp=$dir/download.tar.gz
    curl -qfsSL -o "$tmp" "https://github.com/rhysd/actionlint/releases/download/v$version/actionlint_${version}_linux_$arch.tar.gz"
    echo "$sum  $tmp" | sha256sum -c - > /dev/null || { echo "actionlint.sh: the SHA-256 of the download is not the pin" >&2; rm -f "$tmp"; exit 76; }
    tar -xzf "$tmp" -C "$dir" actionlint
    rm -f "$tmp"
fi
if [ "$arch" = amd64 ]; then
    # Download the pinned ShellCheck, then give its binary to actionlint.
    sh "$root/scripts/shellcheck.sh" --version > /dev/null
    exec "$bin" -shellcheck "$root/.work/tools/shellcheck-v0.11.0/shellcheck" "$@"
fi
exec "$bin" "$@"
