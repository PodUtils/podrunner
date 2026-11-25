#!/bin/sh
# SPDX-License-Identifier: 0BSD
# Install the static Docker Engine used by the full runner image (T-672).
set -eu
version=29.9.2
case "$1" in
    amd64)
        arch=x86_64
        sha=9ac225999fd5e863f7694c728ef23d480616b1e5639328fa309befd4243d69eb
        ;;
    arm64)
        arch=aarch64
        sha=873b3e0207857c3b2f28392fb922f3ea6efe37620c16d4d4aaf2fef409c90b73
        ;;
    *) printf 'install-engine: unsupported architecture %s\n' "$1" >&2; exit 64 ;;
esac
dest=$2
archive=$(mktemp)
trap 'rm -f -- "$archive"' EXIT
curl -fsSLo "$archive" "https://download.docker.com/linux/static/stable/$arch/docker-$version.tgz"
printf '%s  %s\n' "$sha" "$archive" | sha256sum --check --strict
install -d -m 0755 -- "$dest"
tar --extract --gzip --file "$archive" --directory "$dest" --strip-components=1 --no-same-owner
