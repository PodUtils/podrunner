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
#       release, the bundles of x86_64 and aarch64, the image archives and
#       the case table of the tier rule (tier-cases.json, T-518), each with
#       its .sha256, into OUT;
#       DIST holds the binaries and the image archives; the pins of
#       podvm, podnet and the runner come from images/ (a test gives
#       another directory in RELEASE_ASSETS_IMAGES)
#   sh scripts/release-assets.sh index VERSION DIR
#       DIR/release-index.json of each asset of DIR (T-562): the tag
#       vVERSION, the commit of HEAD and its date, written by
#       release-index.py of podcrates (images/pod-release.toml); run it
#       before sums, so that the signature of SHA256SUMS signs it too
#   sh scripts/release-assets.sh sums DIR
#       DIR/SHA256SUMS: one line for each asset of DIR (not the .sha256
#       files), the file that release.yml signs with cosign, as the other
#       projects of PodUtils do (T-484)
#   sh scripts/release-assets.sh check DIR
#       pod-release sums --all DIR: each file and the index against
#       SHA256SUMS, with no signature; a copy of DIR with one changed byte
#       of the index must fail with 77 (T-562)
#   sh scripts/release-assets.sh verify TAG DIR
#       pod-release verify of the signed DIR (SHA256SUMS.sigstore.json) as
#       release.yml of GITHUB_REPOSITORY at TAG; another tag and a changed
#       SHA256SUMS must fail with 77 (T-562)
#
# RELEASE_WORKFLOW names the workflow that signs (default release.yml):
# index writes it into the identity of the index, and verify checks it.
# The runner image release (T-499) is runner-image-release.yml.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
workflow=${RELEASE_WORKFLOW:-release.yml}
die() {
    echo "release-assets: $*" >&2
    exit 1
}
usage() {
    sed -n '/^#   sh /,/^set -eu/p' "$0" | sed '$d' >&2
    exit 64
}
# The script and the verifier of the release index of podcrates, fetched
# once into RELEASE_TOOLS (default: a directory of the temporary space) and
# checked against images/pod-release.toml; prints the directory.
podcrates_tools() {
    tools=${RELEASE_TOOLS:-${RUNNER_TEMP:-${TMPDIR:-/tmp}}/podcrates-release-tools}
    pins=$root/images/pod-release.toml
    arch=$(uname -m)
    pr_version='' pr_commit='' pr_script='' pr_bin=''
    # A failure of the read (no pin for this machine) stops here, not at a
    # download that cannot exist.
    vars=$(python3 -I - "$pins" "$arch" <<'PY'
import sys, tomllib
p = tomllib.load(open(sys.argv[1], "rb"))
print(f"pr_version={p['version']} pr_commit={p['commit']} pr_script={p['script_sha256']} pr_bin={p['sha256'][sys.argv[2]]}")
PY
) || die "$pins: no pin of pod-release for $arch"
    eval "$vars"
    mkdir -p "$tools"
    if ! echo "$pr_script  $tools/release-index.py" | sha256sum -c --status - 2> /dev/null; then
        sh "$root/scripts/retry.sh" curl -qfsSLo "$tools/release-index.py" \
            "https://raw.githubusercontent.com/PodUtils/podcrates/$pr_commit/scripts/release-index.py" >&2
        echo "$pr_script  $tools/release-index.py" | sha256sum -c --status - || die "release-index.py: not the pinned SHA-256"
    fi
    if ! echo "$pr_bin  $tools/pod-release" | sha256sum -c --status - 2> /dev/null; then
        sh "$root/scripts/retry.sh" curl -qfsSLo "$tools/pod-release" \
            "https://github.com/PodUtils/podcrates/releases/download/v$pr_version/pod-release-$arch-unknown-linux-musl" >&2
        echo "$pr_bin  $tools/pod-release" | sha256sum -c --status - || die "pod-release: not the pinned SHA-256"
    fi
    chmod 0755 "$tools/pod-release"
    echo "$tools"
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
        # The case table of the tier rule, for a forge that runs it against
        # its own rule (T-518); a test of podrunner-tier holds it in step.
        cp crates/podrunner-tier/tier-cases.json "$out/tier-cases.json"
        (cd "$out" && sha256sum podrunner-install.sh > podrunner-install.sh.sha256 &&
            sha256sum podrunner-manifest.json > podrunner-manifest.json.sha256 &&
            sha256sum tier-cases.json > tier-cases.json.sha256 && sha256sum -c ./*.sha256)
        # A release asset has at most 2 GiB.
        for f in "$out"/*; do
            size=$(stat -c %s "$f")
            echo "$(basename "$f"): $((size >> 20)) MiB"
            [ "$size" -lt 2147483648 ] || die "$f is 2 GiB or more: GitHub refuses it"
        done
        ;;
    index)
        [ $# -eq 2 ] || usage
        version=$1 dir=$2
        tools=$(podcrates_tools)
        (
            cd "$dir"
            set --
            for f in *; do
                case $f in *.sha256 | SHA256SUMS | SHA256SUMS.sigstore.json | release-index.json) continue ;; esac
                [ -f "$f" ] && set -- "$@" "$f"
            done
            [ $# -gt 0 ] || die "$dir holds no asset"
            commit=$(git -C "$root" rev-parse HEAD)
            date=$(git -C "$root" log -1 --format=%ct HEAD)
            python3 -I "$tools/release-index.py" --project "${GITHUB_REPOSITORY:-PodUtils/podrunner}" \
                --version "v$version" --dir . --date "$date" --commit "$commit" --workflow "$workflow" \
                --out release-index.json "$@"
        )
        echo "release-index.json: v$version, $(grep -c '"sha256"' "$dir/release-index.json") files"
        ;;
    check)
        [ $# -eq 1 ] || usage
        dir=$1
        tools=$(podcrates_tools)
        "$tools/pod-release" sums --all "$dir"
        # The planted defect: one changed byte of the index fails the check.
        tmp=$(mktemp -d)
        cp -p "$dir/SHA256SUMS" "$dir/release-index.json" "$tmp/"
        while IFS= read -r line; do
            name=${line#*  }
            [ "$name" = release-index.json ] || ln -s "$(cd "$dir" && pwd)/$name" "$tmp/$name"
        done < "$dir/SHA256SUMS"
        printf x >> "$tmp/release-index.json"
        # 77 is the refusal; another exit (a link that does not resolve, a
        # read error) proves nothing of the check of the index.
        rc=0
        "$tools/pod-release" sums --all "$tmp" > /dev/null 2>&1 || rc=$?
        rm -rf "$tmp"
        [ "$rc" -eq 77 ] || die "a changed release-index.json gave exit $rc of pod-release sums, not 77"
        echo "pod-release sums: each file and the index hold; a changed index fails (77)"
        ;;
    verify)
        [ $# -eq 2 ] || usage
        tag=$1 dir=$2
        tools=$(podcrates_tools)
        repo=${GITHUB_REPOSITORY:-PodUtils/podrunner}
        "$tools/pod-release" verify --repo "$repo" --workflow "$workflow" --tag "$tag" --all "$dir"
        # The planted defects: another tag, and a changed SHA256SUMS, give 77.
        rc=0
        "$tools/pod-release" verify --repo "$repo" --workflow "$workflow" --tag v0.0.0 --all "$dir" > /dev/null 2>&1 || rc=$?
        [ "$rc" -eq 77 ] || die "the tag v0.0.0 gave exit $rc of pod-release verify, not 77"
        tmp=$(mktemp -d)
        for f in "$dir"/*; do
            [ "${f##*/}" = SHA256SUMS ] || ln -s "$(cd "$dir" && pwd)/${f##*/}" "$tmp/${f##*/}"
        done
        awk 'NR == 1 { $0 = (substr($0, 1, 1) == "0" ? "1" : "0") substr($0, 2) } { print }' "$dir/SHA256SUMS" > "$tmp/SHA256SUMS"
        rc=0
        "$tools/pod-release" verify --repo "$repo" --workflow "$workflow" --tag "$tag" --all "$tmp" > /dev/null 2>&1 || rc=$?
        rm -rf "$tmp"
        [ "$rc" -eq 77 ] || die "a changed SHA256SUMS gave exit $rc of pod-release verify, not 77"
        echo "pod-release verify: $repo $workflow $tag holds; another tag and a changed SHA256SUMS fail (77)"
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
