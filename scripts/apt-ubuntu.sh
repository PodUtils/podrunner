#!/bin/sh
# SPDX-License-Identifier: 0BSD
# Run apt-get update or install with the sources of Ubuntu only. A hosted
# image has third-party sources (for example packages.microsoft.com); when
# one fails, a plain `apt-get update` fails and a step fails with it.
#
#   sh scripts/apt-ubuntu.sh [--keep FILE]... update
#   sh scripts/apt-ubuntu.sh [--keep FILE]... install PACKAGE...
#
# The Ubuntu sources are /etc/apt/sources.list (one-line style) and
# /etc/apt/sources.list.d/ubuntu.sources (deb822 style, Ubuntu 24.04).
# apt reads deb822 files from the source parts directory only, so the
# script copies these files to a new directory and points
# Dir::Etc::sourceparts at it. --keep FILE adds a source that the step
# made itself (a .list or .sources file); the step passes it to update and
# to install. The network retry stays with the caller (scripts/retry.sh).
# APT_UBUNTU_ETC (default /etc/apt) and APT_UBUNTU_SUDO (default sudo
# when not root) are for the self-test.
set -eu
usage() { echo "usage: apt-ubuntu.sh [--keep FILE]... update | install PACKAGE..." >&2; exit 2; }
etc=${APT_UBUNTU_ETC:-/etc/apt}
if [ "${APT_UBUNTU_SUDO+set}" = set ]; then
    sudo_cmd=$APT_UBUNTU_SUDO
elif [ "$(id -u)" -eq 0 ]; then
    sudo_cmd=
else
    sudo_cmd=sudo
fi
dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT HUP INT TERM
n=0
while [ "$#" -gt 0 ] && [ "$1" = --keep ]; do
    [ "$#" -ge 2 ] && [ -f "$2" ] || { echo "apt-ubuntu.sh: --keep needs a file" >&2; exit 2; }
    case $2 in *.list | *.sources) ;; *) echo "apt-ubuntu.sh: --keep file is not .list or .sources: $2" >&2; exit 2 ;; esac
    n=$((n + 1))
    cp "$2" "$dir/keep$n-$(basename "$2")"
    shift 2
done
[ "$#" -gt 0 ] || usage
verb=$1
shift
if [ -f "$etc/sources.list.d/ubuntu.sources" ]; then
    cp "$etc/sources.list.d/ubuntu.sources" "$dir/ubuntu.sources"
fi
list=$etc/sources.list
[ -f "$list" ] || list=/dev/null
# apt needs one Ubuntu source at least.
if [ ! -f "$dir/ubuntu.sources" ] && ! grep -q '^[[:space:]]*deb' "$list"; then
    echo "apt-ubuntu.sh: no Ubuntu source in $etc" >&2
    exit 1
fi
chmod -R a+rX "$dir"
case $verb in
update) extra= ;;
install) extra="-y --no-install-recommends" ;;
*) usage ;;
esac
# $extra has no spaces inside a word, so it splits as intended.
# shellcheck disable=SC2086
$sudo_cmd apt-get -qq -o "Dir::Etc::sourceparts=$dir" -o "Dir::Etc::sourcelist=$list" $extra "$verb" "$@"
