#!/bin/sh
# SPDX-License-Identifier: 0BSD
# The offline bundle of a release for one architecture (T-202): one
# .tar.gz with each file that the installer uses, the installer, a
# SHA256SUMS and the marker PODRUNNER-BUNDLE. The pins come from the block
# of pins of INSTALLER (a copy that scripts/install-pins.py stamped for the
# release); each file must agree with its pin.
#
#   sh scripts/make-bundle.sh INSTALLER ARCH ASSETS OUT
#
# ASSETS holds podrunner-ARCH-unknown-linux-musl, runner-ARCH.tar and
# podrunner.1 (when the pins name it) of the release. podvm, podnet and the
# runner tarball come from ASSETS when they are there, else from their
# releases (or PODRUNNER_MIRROR=URL). The bundle is
# OUT/podrunner-bundle-ARCH.tar.gz, with its .sha256. The larger image is
# not in the bundle: it is a release asset of its own.
set -eu
[ $# -eq 4 ] || {
    echo "usage: sh scripts/make-bundle.sh INSTALLER ARCH ASSETS OUT" >&2
    exit 64
}
installer=$1 arch=$2 assets=$3 out=$4
case $arch in
    x86_64) rarch=x64 ;;
    aarch64) rarch=arm64 ;;
    *) echo "make-bundle: no architecture $arch" >&2; exit 64 ;;
esac
pins=$(mktemp)
trap 'rm -f "$pins"' EXIT
sed -n '/^# BEGIN PINS/,/^# END PINS/p' "$installer" > "$pins"
# shellcheck disable=SC1090
. "$pins"
[ -n "$PODRUNNER_VERSION" ] || { echo "make-bundle: $installer has no pins of a release" >&2; exit 1; }
pin() { eval "printf '%s' \"\${$1_$arch:-}\""; }

dir=podrunner-bundle-$PODRUNNER_VERSION-$arch
mkdir -p "$out"
rm -rf "${out:?}/$dir"
mkdir "$out/$dir"

# add KIND NAME URL: the file from ASSETS, else from URL; its SHA-256 must
# be the pin.
add() {
    if [ -f "$assets/$2" ]; then
        cp "$assets/$2" "$out/$dir/$2"
    else
        url=$3
        [ -z "${PODRUNNER_MIRROR:-}" ] || url="${PODRUNNER_MIRROR%/}/$2"
        case $url in
            file://*) cp "${url#file://}" "$out/$dir/$2" ;;
            *) curl -qfsSL --retry 3 -o "$out/$dir/$2" "$url" ;;
        esac
    fi
    have=$(sha256sum < "$out/$dir/$2" | cut -d' ' -f1)
    case $1 in manpage) want=$SUM_manpage ;; *) want=$(pin "SUM_$1") ;; esac
    [ "$have" = "$want" ] || {
        echo "make-bundle: $2: the SHA-256 is $have, not the pin $want" >&2
        exit 1
    }
    echo "make-bundle: ok $2"
}
gh=https://github.com
add podrunner "podrunner-$arch-unknown-linux-musl" ""
add podvm "podvm-$arch" "$gh/PodUtils/podvm/releases/download/v$PODVM_VERSION/podvm-$arch"
add podnet "podnet-$arch-unknown-linux-musl" \
    "$gh/PodUtils/podnet/releases/download/v$PODNET_VERSION/podnet-$arch-unknown-linux-musl"
tgz=actions-runner-linux-$rarch-$RUNNER_VERSION.tar.gz
add runner "$tgz" "$gh/actions/runner/releases/download/v$RUNNER_VERSION/$tgz"
add image "runner-$arch.tar" ""
# The man page (T-692): one file for each release. A release before T-645
# has none and no pin of it: the bundle has no page, and the installer of the
# bundle skips it.
if [ -n "$SUM_manpage" ]; then
    add manpage podrunner.1 ""
else
    echo "make-bundle: skip podrunner.1: the pins have none"
fi
cp "$installer" "$out/$dir/podrunner-install.sh"
# The licences: podrunner (0BSD), the list of third-party components, and
# the licence of the official runner, whose tarball has no licence file.
src=$(cd "$(dirname "$0")/.." && pwd)
cp "$src/LICENSE" "$src/THIRD_PARTY.md" "$src/images/runner/LICENSE.actions-runner" "$out/$dir/"
chmod 0755 "$out/$dir/podrunner-install.sh" "$out/$dir/podrunner-$arch-unknown-linux-musl" "$out/$dir/podvm-$arch" \
    "$out/$dir/podnet-$arch-unknown-linux-musl"
cat > "$out/$dir/PODRUNNER-BUNDLE" <<EOF
podrunner $PODRUNNER_VERSION, $arch: the offline bundle of the local mode.
Install with no network:  sh podrunner-install.sh
For the larger image (--full), put runner-full-$arch.tar of the release
into this directory first.
EOF
(cd "$out/$dir" && sha256sum -- * > SHA256SUMS)
tar -cf - -C "$out" "$dir" | gzip -6 > "$out/podrunner-bundle-$arch.tar.gz"
rm -rf "${out:?}/$dir"
(cd "$out" && sha256sum "podrunner-bundle-$arch.tar.gz" > "podrunner-bundle-$arch.tar.gz.sha256")
echo "make-bundle: $out/podrunner-bundle-$arch.tar.gz ($(du -m "$out/podrunner-bundle-$arch.tar.gz" | cut -f1) MiB)"
