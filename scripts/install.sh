#!/bin/sh
# shellcheck disable=SC2034 # pin() reads the block of pins by name.
# SPDX-License-Identifier: 0BSD
# The installer of the local mode of podrunner (T-202). It installs
# podrunner, podvm, podnet, the official runner and the runner image into a
# podvm store, and writes the file of the user `run.toml`, so that
# `podrunner run FILE --checkout .` needs no other flag. It needs no root,
# no token and no Python: /bin/sh, curl or wget, sha256sum or shasum or
# openssl, tar and gzip.
#
#   sh podrunner-install.sh [--prefix DIR] [--full] [--bundle PATH]
#
# Each version and each SHA-256 is in the block of pins below. A release
# writes the block (scripts/install-pins.py); a copy of the tree has no
# pins of a release and installs only from a bundle of a release. A second
# run checks what exists and downloads only what is missing or wrong. A
# file goes to its place only after its SHA-256 agrees with its pin.
set -eu
# The directories and files of the install are not writable by the group
# or others, whatever the umask of the user: podrunner refuses a binary in a
# directory that others can write (the user-path gate, T-253).
umask 022

# BEGIN PINS (scripts/install-pins.py writes this block; do not edit it)
PODRUNNER_VERSION=""
PODVM_VERSION="0.2.0-beta.34"
PODNET_VERSION="0.4.0-beta.17"
RUNNER_VERSION="2.338.2"
SUM_podrunner_x86_64=""
SUM_podrunner_aarch64=""
SUM_podvm_x86_64="2ca17f08ddb8707ca045b664712cf53b3e1429aab8701ef9e7ac09a5fc6dc4b1"
SUM_podvm_aarch64="4561f48dd3cd4534541d6ce452b1dd489e544c73508d8ed55baa6eb2df73ab68"
SUM_podnet_x86_64="d0b72b9bfe2473afef360e41c8a830a003239c8695535bef0f4bbd20486ab1f5"
SUM_podnet_aarch64="e8c37a1d100d169c6e3240b2df33ffb9ec584d94c47ca959a970d8df7f327d71"
SUM_runner_x86_64="baebf204307bac6193480852732f0aba2ae97b6c5597c244f4d5558390147c40"
SUM_runner_aarch64="670adb2f268fe21dcbb58b3db0e0aa9d5b3bddfe2c651481336e561ff86de945"
SUM_image_x86_64=""
SUM_image_aarch64=""
DIGEST_image_x86_64=""
DIGEST_image_aarch64=""
SUM_imagefull_x86_64=""
SUM_imagefull_aarch64=""
DIGEST_imagefull_x86_64=""
DIGEST_imagefull_aarch64=""
# END PINS

usage() {
    cat <<'EOF'
usage: sh podrunner-install.sh [--prefix DIR] [--full] [--bundle PATH]

  --prefix DIR   install podrunner into DIR/bin and the data into
                 DIR/share/podrunner (default: ~/.local, and the data into
                 $XDG_DATA_HOME/podrunner when it is set)
  --full         load the larger runner image (the common tools of
                 ubuntu-latest) and use it for each job
  --bundle PATH  install from a bundle of a release (its .tar.gz or its
                 unpacked directory), with no network

The environment variable PODRUNNER_MIRROR=URL takes each file from URL/NAME
in place of its release on github.com. Each file must agree with its pin.
EOF
}

say() { printf '%s\n' "podrunner-install: $*"; }
die() {
    printf '%s\n' "podrunner-install: error: $*" >&2
    exit 1
}
have() { command -v "$1" > /dev/null 2>&1; }

prefix="" full=0 bundle=""
while [ $# -gt 0 ]; do
    case $1 in
        --prefix) [ $# -ge 2 ] || die "--prefix needs a directory"; prefix=$2; shift 2 ;;
        --prefix=*) prefix=${1#--prefix=}; shift ;;
        --full) full=1; shift ;;
        --bundle) [ $# -ge 2 ] || die "--bundle needs a path"; bundle=$2; shift 2 ;;
        --bundle=*) bundle=${1#--bundle=}; shift ;;
        -h | --help) usage; exit 0 ;;
        *) usage >&2; die "unknown option: $1" ;;
    esac
done

# The architecture names of the assets.
case $(uname -m) in
    x86_64 | amd64) arch=x86_64 rarch=x64 ;;
    aarch64 | arm64) arch=aarch64 rarch=arm64 ;;
    *) die "the architecture $(uname -m) has no release: podrunner has x86_64 and aarch64" ;;
esac
[ "$(uname -s)" = Linux ] || die "podrunner runs on Linux only, not on $(uname -s)"
[ -n "${HOME:-}" ] || die "HOME is not set"

pin() { eval "printf '%s' \"\${$1_$arch:-}\""; }

sha256_of() {
    if have sha256sum; then
        sha256sum < "$1" | cut -d' ' -f1
    elif have shasum; then
        shasum -a 256 < "$1" | cut -d' ' -f1
    elif have openssl; then
        openssl dgst -sha256 -r < "$1" | cut -d' ' -f1
    else
        die "no sha256sum, shasum or openssl: install one of them"
    fi
}

have tar || die "no tar: install tar"
have gzip || die "no gzip: install gzip"
sha256_of /dev/null > /dev/null

# A bundle: the unpacked directory, or its archive. The directory of this
# script is a bundle when it holds the marker file.
here=$(cd "$(dirname "$0")" && pwd)
if [ -z "$bundle" ] && [ -f "$here/PODRUNNER-BUNDLE" ]; then
    bundle=$here
fi

if [ -z "$PODRUNNER_VERSION" ] || [ -z "$(pin SUM_podrunner)" ] || [ -z "$(pin SUM_image)" ]; then
    die "this copy of the installer has no pins of a release for $arch: take podrunner-install.sh or the bundle of a release of PodUtils/podrunner"
fi

if [ -n "$prefix" ]; then
    mkdir -p "$prefix" || die "cannot make $prefix"
    prefix=$(cd "$prefix" && pwd)
    data=$prefix/share/podrunner
else
    prefix=$HOME/.local
    data=${XDG_DATA_HOME:-$HOME/.local/share}/podrunner
fi
bin=$prefix/bin
config=${XDG_CONFIG_HOME:-$HOME/.config}/podrunner
store=$data/images
downloads=$data/downloads
mkdir -p "$bin" "$data/bin" "$store" "$downloads" "$config" || die "cannot make the directories under $prefix"
# The umask does not apply to a directory that was there before: podrunner
# refuses a driver in a directory that the group or the others can write.
chmod go-w "$bin" "$data/bin" || die "cannot remove the write of the group and the others from $bin and $data/bin"
# A partial file of an earlier run that stopped is never used.
rm -f "$bin/podrunner.part" "$data"/bin/*.part "$downloads"/*.part

unpacked=0
if [ -n "$bundle" ] && [ -f "$bundle" ]; then
    # Unpack the archive of a bundle once; a second run unpacks it again.
    rm -rf "$downloads/bundle"
    mkdir -p "$downloads/bundle"
    gzip -dc < "$bundle" | tar -xf - -C "$downloads/bundle" || die "$bundle: not a bundle (.tar.gz)"
    set -- "$downloads"/bundle/*/PODRUNNER-BUNDLE
    [ -f "$1" ] || die "$bundle: no PODRUNNER-BUNDLE in it: not a bundle of a release"
    bundle=$(dirname "$1")
    unpacked=1
elif [ -n "$bundle" ]; then
    [ -f "$bundle/PODRUNNER-BUNDLE" ] || die "$bundle: no PODRUNNER-BUNDLE in it: not a bundle of a release"
fi
if [ -z "$bundle" ] && [ -z "${PODRUNNER_MIRROR:-}" ] && ! have curl && ! have wget; then
    die "no curl and no wget: install one of them, or give --bundle PATH"
fi

# The place of each file on the network.
url_of() {
    if [ -n "${PODRUNNER_MIRROR:-}" ]; then
        printf '%s/%s' "${PODRUNNER_MIRROR%/}" "$2"
        return
    fi
    case $1 in
        podrunner | image | imagefull) printf 'https://github.com/PodUtils/podrunner/releases/download/v%s/%s' "$PODRUNNER_VERSION" "$2" ;;
        podvm) printf 'https://github.com/PodUtils/podvm/releases/download/v%s/%s' "$PODVM_VERSION" "$2" ;;
        podnet) printf 'https://github.com/PodUtils/podnet/releases/download/v%s/%s' "$PODNET_VERSION" "$2" ;;
        runner) printf 'https://github.com/actions/runner/releases/download/v%s/%s' "$RUNNER_VERSION" "$2" ;;
    esac
}

fetch() {
    case $1 in
        file://*) cp "${1#file://}" "$2" ;;
        *)
            if have curl; then
                curl -qfsSL --retry 3 --proto '=https,http' -o "$2" "$1"
            else
                wget -q -O "$2" "$1"
            fi
            ;;
    esac
}

# get KIND NAME DEST: the file NAME, checked against its pin, at DEST. A
# file at DEST that agrees with the pin stays; a partial or wrong file never
# takes the place of DEST.
got=0
get() {
    want=$(pin "SUM_$1")
    [ -n "$want" ] || die "$2: no pin for $arch in this installer"
    if [ -f "$3" ] && [ "$(sha256_of "$3")" = "$want" ]; then
        say "ok      $2 (present)"
        return 0
    fi
    part="$3.part"
    rm -f "$part"
    if [ -n "$bundle" ]; then
        [ -f "$bundle/$2" ] || die "$2: not in the bundle $bundle (download it from the release into that directory)"
        cp "$bundle/$2" "$part" || die "$2: cannot copy it from the bundle"
    else
        say "fetch   $2"
        fetch "$(url_of "$1" "$2")" "$part" || {
            rm -f "$part"
            die "$2: the download from $(url_of "$1" "$2") failed"
        }
    fi
    have_sum=$(sha256_of "$part")
    if [ "$have_sum" != "$want" ]; then
        rm -f "$part"
        die "$2: the SHA-256 is $have_sum, not the pin $want: the file is wrong or partial"
    fi
    mv -f "$part" "$3"
    got=$((got + 1))
    say "ok      $2 (sha256 $want)"
}

# The binaries.
get podrunner "podrunner-$arch-unknown-linux-musl" "$bin/podrunner"
chmod 0755 "$bin/podrunner"
get podvm "podvm-$arch" "$data/bin/podvm"
chmod 0755 "$data/bin/podvm"
get podnet "podnet-$arch-unknown-linux-musl" "$data/bin/podnet"
chmod 0755 "$data/bin/podnet"

# The official runner, unpacked, for --runner-dir. The marker holds the
# SHA-256 of its tarball; a run may change the directory, so only the
# marker is checked.
runner_dir=$data/runner/$RUNNER_VERSION
runner_sum=$(pin SUM_runner)
if [ -f "$runner_dir/.podrunner-install" ] && [ "$(cat "$runner_dir/.podrunner-install")" = "$runner_sum" ]; then
    say "ok      the runner $RUNNER_VERSION in $runner_dir (present)"
else
    tgz="actions-runner-linux-$rarch-$RUNNER_VERSION.tar.gz"
    get runner "$tgz" "$downloads/$tgz"
    tmp=$data/runner/.unpack.$$
    rm -rf "$tmp" "$runner_dir"
    mkdir -p "$tmp"
    gzip -dc < "$downloads/$tgz" | tar -xf - -C "$tmp" || {
        rm -rf "$tmp"
        die "$tgz: the unpack failed"
    }
    printf '%s\n' "$runner_sum" > "$tmp/.podrunner-install"
    mv "$tmp" "$runner_dir"
    rm -f "$downloads/$tgz"
    say "ok      the runner $RUNNER_VERSION in $runner_dir"
fi

# The runner image in the podvm store. The store checks each blob against
# its digest at the load; `podvm verify` checks them again on each run.
podvm=$data/bin/podvm
load_image() { # KIND NAME
    digest=$(pin "DIGEST_$1")
    if [ -z "$digest" ] || [ -z "$(pin "SUM_$1")" ]; then
        die "$2: this release has no such image for $arch"
    fi
    if PODVM_STORE=$store "$podvm" inspect "runner@$digest" > /dev/null 2>&1; then
        PODVM_STORE=$store "$podvm" verify > /dev/null 2>&1 ||
            die "the store $store fails 'podvm verify': remove $store and run the installer again"
        say "ok      runner@$digest in $store (present)"
    else
        get "$1" "$2" "$downloads/$2"
        PODVM_STORE=$store "$podvm" load -i "$downloads/$2" > /dev/null || die "$2: podvm load failed"
        PODVM_STORE=$store "$podvm" inspect "runner@$digest" > /dev/null 2>&1 ||
            die "$2: the store has no runner@$digest after the load"
        rm -f "$downloads/$2"
        say "ok      runner@$digest in $store"
    fi
}
if [ "$full" = 1 ]; then
    load_image imagefull "runner-full-$arch.tar"
    image=$(pin DIGEST_imagefull)
else
    load_image image "runner-$arch.tar"
    image=$(pin DIGEST_image)
fi
[ "$unpacked" = 0 ] || rm -rf "$downloads/bundle"

# A small workflow for a first run.
cat > "$data/hello.yml" <<'EOF'
on: push
jobs:
  hello:
    runs-on: ubuntu-latest
    steps:
      - run: echo "hello from a podrunner unit"
EOF

# The file of the user: each key is a flag of `podrunner run` (T-143). A
# file that this installer did not write stays as it is.
marker="# podrunner-install: this installer writes this file again on each run; remove this line to keep your changes."
toml=$config/run.toml
cat > "$toml.new" <<EOF
$marker
# Each key is a flag of \`podrunner run\` with no -- (docs/cli.md).
podvm = "$podvm"
images = "$store"
image = "runner@$image"
# The egress hold: remove the # of the next two lines, and give each host
# that a job needs (for example a registry of packages) in allow.
# podnet = "$data/bin/podnet"
# allow = ["github.com"]
# The executor with no unit: put a # before podvm, images and image, and
# remove the # of the next line.
# runner-dir = "$runner_dir"
EOF
if [ ! -f "$toml" ] || [ "$(head -n 1 "$toml")" = "$marker" ]; then
    mv -f "$toml.new" "$toml"
    say "ok      $toml"
else
    mv -f "$toml.new" "$toml.podrunner-install"
    say "keep    $toml: you wrote it; the keys of this install are in $toml.podrunner-install"
fi

# What works on this host.
say "the checks of podrunner doctor:"
"$bin/podrunner" doctor 2>&1 | grep -v '^missing: --config' | sed 's/^/    /' || true
"$bin/podrunner" doctor --run "$data/hello.yml" 2>&1 | sed 's/^/    /' || true
doc=$("$bin/podrunner" doctor 2>&1 || true)
case $doc in *"probe unshare(CLONE_NEWUSER): ok"*) userns=yes ;; *) userns=no ;; esac
case $doc in *"kvm: no"* | *"kvm: denied"*) kvm=no ;; *) kvm=yes ;; esac
say "user namespaces: $userns (podvm takes its namespace rung with them: see docs/local-mode.md, What a podvm unit holds)"
say "/dev/kvm: $kvm (the --microvm kind needs it)"
[ "$got" -eq 0 ] && say "nothing was downloaded: each file was present and correct"

case ":$PATH:" in
    *":$bin:"*) run=podrunner ;;
    *) run="$bin/podrunner"; say "$bin is not in PATH: add it, or use the full path" ;;
esac
say "a first run:  $run run $data/hello.yml"
say "next, in the tree of your project:  $run run .github/workflows/ci.yml --checkout ."
