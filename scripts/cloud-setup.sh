#!/bin/sh
# SPDX-License-Identifier: 0BSD
# Install the pinned tools on a Linux host. Load cloud-env.sh in each shell.
#   sh scripts/cloud-setup.sh [--runner] [--dev-tools] [TARGET...]
#
# With --runner, also unpack the pinned official Actions runner for check.sh.
# With --dev-tools, also install missing groff-base, mandoc and
# python3-jsonschema packages when root or passwordless sudo apt-get works.
set -eu

with_runner=0
dev_tools=0
while [ $# -gt 0 ]; do
    case "$1" in
        --runner)
            with_runner=1
            shift
            ;;
        --dev-tools)
            dev_tools=1
            shift
            ;;
        -h | --help)
            sed -n '3,8p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        --)
            shift
            break
            ;;
        -*)
            echo "usage: sh scripts/cloud-setup.sh [--runner] [--dev-tools] [TARGET...]" >&2
            exit 64
            ;;
        *)
            break
            ;;
    esac
done

CDPATH='' cd "$(dirname "$0")/.."
# shellcheck source=scripts/cloud-env.sh
. ./scripts/cloud-env.sh
umask 077

[ "$(uname -s)" = Linux ] || { echo "cloud-setup: Linux only" >&2; exit 2; }
case $(uname -m) in
    x86_64)
        arch=x86_64
        runner_arch=x64
        zig_sha=70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00
        rustup_sha=b1c8cb14e30889e7e2999e6b69e62d9e9b9ffc0f4df8180564d306d9680e6c55
        ;;
    aarch64)
        arch=aarch64
        runner_arch=arm64
        zig_sha=ea4b09bfb22ec6f6c6ceac57ab63efb6b46e17ab08d21f69f3a48b38e1534f17
        rustup_sha=9f608c142e0e38a78957d0661ba1dfaf0b6fb37184cfaa4b9a0aa57af6184282
        ;;
    *) echo "cloud-setup: unsupported host architecture" >&2; exit 2 ;;
esac
[ "$#" -gt 0 ] || set -- x86_64-unknown-linux-musl

for tool in cc curl file flock git python3 readelf sha256sum tar timeout xz; do
    command -v "$tool" > /dev/null || { echo "cloud-setup: install $tool" >&2; exit 1; }
done
mkdir -p "$PODRUNNER_TOOLS/bin"
exec 9>"$PODRUNNER_TOOLS/setup.lock"
flock -w 600 9

stage=$(mktemp -d "$PODRUNNER_TOOLS/setup.XXXXXXXX")
trap 'rm -rf -- "$stage"' EXIT
trap 'exit 1' HUP INT TERM
download() {
    curl -qfsSL --proto '=https' --proto-redir '=https' --retry 2 --max-time 180 -o "$2" "$1" </dev/null
    printf '%s  %s\n' "$3" "$2" | sha256sum -c -
}

install_dev_tools() (
    umask 022
    missing=
    if ! command -v groff >/dev/null 2>&1; then
        missing="$missing groff-base"
    fi
    if ! command -v mandoc >/dev/null 2>&1; then
        missing="$missing mandoc"
    fi
    if ! python3 -I -c 'import jsonschema' 2>/dev/null &&
       ! { [ -x /usr/bin/python3 ] && /usr/bin/python3 -I -c 'import jsonschema' 2>/dev/null; }; then
        missing="$missing python3-jsonschema"
    fi
    if [ -n "$missing" ] && command -v apt-get >/dev/null 2>&1; then
        apt_sudo=none
        if [ "$(id -u)" = 0 ]; then
            apt_sudo=
        elif command -v sudo >/dev/null 2>&1 && sudo -n -E true 2>/dev/null; then
            apt_sudo="sudo -n -E"
        elif command -v sudo >/dev/null 2>&1 &&
             (sudo -n true 2>/dev/null || sudo -n apt-get --version >/dev/null 2>&1); then
            apt_sudo="sudo -n"
        fi
        if [ "$apt_sudo" != none ]; then
            if [ -f /etc/apt/sources.list.d/ubuntu.sources ] ||
               { [ -f /etc/apt/sources.list ] && grep -q '^[[:space:]]*deb' /etc/apt/sources.list; }; then
                # shellcheck disable=SC2086 # $missing is a package word list
                DEBIAN_FRONTEND=noninteractive APT_UBUNTU_SUDO="$apt_sudo" timeout 300 sh scripts/apt-ubuntu.sh update </dev/null &&
                    DEBIAN_FRONTEND=noninteractive APT_UBUNTU_SUDO="$apt_sudo" timeout 300 sh scripts/apt-ubuntu.sh install $missing </dev/null
            fi
        fi
    fi
)

if ! command -v rustup >/dev/null 2>&1; then
    url=https://static.rust-lang.org/rustup/archive/1.29.3/$arch-unknown-linux-gnu/rustup-init
    download "$url" "$stage/rustup-init" "$rustup_sha"
    chmod u+x "$stage/rustup-init"
    timeout 60 "$stage/rustup-init" -y --no-modify-path --default-toolchain none </dev/null
fi
# Rustup reads the channel, components and default targets from rust-toolchain.toml.
if ! RUSTUP_AUTO_INSTALL=0 rustup which cargo >/dev/null 2>&1; then
    timeout 600 rustup toolchain install --no-self-update </dev/null
fi
timeout 600 rustup target add "$@" </dev/null

zig_version=${ZIG_VERSION:-0.16.0}
zig_sha=${ZIG_SHA256:-$zig_sha}
zigbuild_version=${ZIGBUILD_VERSION:-0.23.4}
if [ "$(zig version 2>/dev/null || :)" != "$zig_version" ]; then
    ZIG_ARCH=$arch sh scripts/fetch-zig.sh "$zig_version" "$zig_sha" "$stage/zig.tar.xz"
    tar -xJf "$stage/zig.tar.xz" -C "$PODRUNNER_TOOLS"
    ln -sf "$PODRUNNER_TOOLS/zig-$arch-linux-$zig_version/zig" "$PODRUNNER_TOOLS/bin/zig"
fi
if [ "$(cargo-zigbuild --version 2>/dev/null || :)" != "cargo-zigbuild $zigbuild_version" ]; then
    timeout 900 cargo install --locked --quiet cargo-zigbuild --version "$zigbuild_version" </dev/null
fi

if [ "$with_runner" -eq 1 ] && [ ! -x "$podrunner_runner_cache/bin/Runner.Listener" ]; then
    version=$podrunner_runner_version
    sum=$(sed -n "s/^$arch = \"\([0-9a-f]*\)\"$/\1/p" images/runner/runner.toml)
    url=https://github.com/actions/runner/releases/download/v$version/actions-runner-linux-$runner_arch-$version.tar.gz
    download "$url" "$stage/runner.tar.gz" "$sum"
    mkdir "$stage/runner"
    tar -xzf "$stage/runner.tar.gz" -C "$stage/runner"
    test -x "$stage/runner/bin/Runner.Listener"
    mv -T "$stage/runner" "$podrunner_runner_cache"
fi

if [ "$dev_tools" -eq 1 ]; then
    install_dev_tools
fi

if ! cargo fetch --locked --offline >/dev/null 2>&1; then
    timeout 600 cargo fetch --locked </dev/null
fi

printf 'cloud-setup: %s, Zig %s, %s\n' "$(rustc --version)" "$(zig version)" "$(cargo-zigbuild --version)"
if [ "$with_runner" -eq 1 ]; then
    printf 'cloud-setup: Actions runner %s in %s\n' "$podrunner_runner_version" "$podrunner_runner_cache"
fi
