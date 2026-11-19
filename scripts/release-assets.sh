#!/bin/sh
# SPDX-License-Identifier: 0BSD
# The assets of a release (T-205). release.yml and user-gate.yml (T-243)
# both call this script, so that the user-path gate tests the files that a
# release publishes, made in the same way.
#
#   sh scripts/release-assets.sh tools
#       the toolchain of rust-toolchain.toml, zig and cargo-zigbuild
#       (ZIG_VERSION, ZIG_SHA256, ZIGBUILD_VERSION from the environment)
#   sh scripts/release-assets.sh binaries TARGET VERSION DIST
#       the static podrunner and scale-set sidecar of TARGET, each with its
#       .sha256, into DIST; VERSION is empty or names the version of
#       Cargo.toml (crates/podrunner/build.rs)
#   sh scripts/release-assets.sh images RUN DIST
#       the runner image archives of the runner-image.yml run RUN (gh and
#       GH_TOKEN), checked against their .sha256 files, into DIST
#   sh scripts/release-assets.sh local-mode VERSION DIST OUT
#       the installer with the pins of VERSION, the manifest of the
#       release, the bundles of x86_64 and aarch64 and the image archives,
#       each with its .sha256, into OUT;
#       DIST holds the binaries and the image archives; the pins of
#       podvm, podnet and the runner come from images/ (a test gives
#       another directory in RELEASE_ASSETS_IMAGES)
#   sh scripts/release-assets.sh sums DIR
#       DIR/SHA256SUMS: one line for each asset of DIR (not the .sha256
#       files), the file that release.yml signs with cosign, as the other
#       projects of PodUtils do (T-484)
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
die() {
    echo "release-assets: $*" >&2
    exit 1
}
usage() {
    sed -n '/^#   sh /,/^set -eu/p' "$0" | sed '$d' >&2
    exit 64
}
[ $# -ge 1 ] || usage
cmd=$1
shift

case $cmd in
    tools)
        [ $# -eq 0 ] || usage
        : "${ZIG_VERSION:?}" "${ZIG_SHA256:?}" "${ZIGBUILD_VERSION:?}"
        tmp=${RUNNER_TEMP:-${TMPDIR:-/tmp}}
        sh "$root/scripts/retry.sh" rustup toolchain install
        mkdir -p "$tmp/zig"
        sh "$root/scripts/fetch-zig.sh" "$ZIG_VERSION" "$ZIG_SHA256" "$tmp/zig.tar.xz"
        tar -xJf "$tmp/zig.tar.xz" -C "$tmp/zig"
        [ -z "${GITHUB_PATH:-}" ] || echo "$tmp/zig/zig-x86_64-linux-$ZIG_VERSION" >> "$GITHUB_PATH"
        sh "$root/scripts/retry.sh" cargo install --locked --quiet cargo-zigbuild --version "$ZIGBUILD_VERSION"
        ;;
    binaries)
        [ $# -eq 3 ] || usage
        target=$1 version=$2 dist=$3
        case $target in
            x86_64-*) goarch=amd64 ;;
            aarch64-*) goarch=arm64 ;;
            *) die "no target $target" ;;
        esac
        mkdir -p "$dist"
        dist=$(cd "$dist" && pwd)
        cd "$root"
        PODRUNNER_VERSION=$version PODRUNNER_COMMIT=$(git rev-parse --short=12 HEAD) \
            cargo zigbuild --locked --release --target "$target" -p podrunner
        name=podrunner-$target
        cp "target/$target/release/podrunner" "$dist/$name"
        sh scripts/check-static.sh "$dist/$name"
        (cd "$dist" && sha256sum "$name" > "$name.sha256")
        # The scale-set sidecar (T-029): static Go, no cgo. go.mod names
        # the toolchain; the go command fetches it.
        name=scaleset-sidecar-$target
        (cd tools/scaleset-sidecar && GOTOOLCHAIN=auto CGO_ENABLED=0 GOARCH=$goarch go build -trimpath -o "$dist/$name" .)
        sh scripts/check-static.sh "$dist/$name"
        (cd "$dist" && sha256sum "$name" > "$name.sha256")
        ;;
    images)
        [ $# -eq 2 ] || usage
        run=$1 dist=$2
        repo=${GITHUB_REPOSITORY:-PodUtils/podrunner}
        mkdir -p "$dist"
        for a in x86_64 aarch64; do
            sh "$root/scripts/retry.sh" gh run download "$run" --repo "$repo" -n "runner-$a" -D "$dist"
            # The larger image of a run before T-202 has no aarch64 archive:
            # then the installer refuses --full on aarch64 by name.
            gh run download "$run" --repo "$repo" -n "runner-full-$a" -D "$dist" || echo "no runner-full-$a in run $run"
        done
        (cd "$dist" && sha256sum -c ./runner-*.sha256)
        ;;
    local-mode)
        [ $# -eq 3 ] || usage
        version=$1
        mkdir -p "$2" "$3"
        dist=$(cd "$2" && pwd) out=$(cd "$3" && pwd)
        cd "$root"
        python3 -I scripts/install-pins.py --check scripts/install.sh
        mkdir -p "$out"
        python3 -I scripts/install-pins.py --images "${RELEASE_ASSETS_IMAGES:-images}" --release "$version" --assets "$dist" \
            scripts/install.sh --out "$out/podrunner-install.sh"
        # The manifest that a machine reads (T-499): the versions, the files
        # and the image digests of this release.
        python3 -I scripts/install-pins.py --images "${RELEASE_ASSETS_IMAGES:-images}" --release "$version" --assets "$dist" \
            --manifest "$out/podrunner-manifest.json"
        for a in x86_64 aarch64; do sh scripts/make-bundle.sh "$out/podrunner-install.sh" "$a" "$dist" "$out"; done
        cp "$dist"/runner-*.tar "$dist"/runner-*.tar.sha256 "$out/"
        (cd "$out" && sha256sum podrunner-install.sh > podrunner-install.sh.sha256 &&
            sha256sum podrunner-manifest.json > podrunner-manifest.json.sha256 && sha256sum -c ./*.sha256)
        # A release asset has at most 2 GiB.
        for f in "$out"/*; do
            size=$(stat -c %s "$f")
            echo "$(basename "$f"): $((size >> 20)) MiB"
            [ "$size" -lt 2147483648 ] || die "$f is 2 GiB or more: GitHub refuses it"
        done
        ;;
    sums)
        [ $# -eq 1 ] || usage
        dir=$1
        (
            cd "$dir"
            rm -f SHA256SUMS SHA256SUMS.sigstore.json
            set --
            for f in *; do
                case $f in *.sha256) continue ;; esac
                [ -f "$f" ] && set -- "$@" "$f"
            done
            [ $# -gt 0 ] || die "$dir holds no asset"
            sha256sum -- "$@" > SHA256SUMS.part
            mv -f SHA256SUMS.part SHA256SUMS
            sha256sum -c --quiet SHA256SUMS
        )
        echo "SHA256SUMS: $(wc -l < "$dir/SHA256SUMS") assets"
        ;;
    *) usage ;;
esac
