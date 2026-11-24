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
# file goes to its place only after its SHA-256 agrees with its pin. A
# download of a file of the podrunner release also needs SHA256SUMS and
# release-index.json of the release to name it (T-523).
set -eu
# The directories and files of the install are not writable by the group
# or others, whatever the umask of the user: podrunner refuses a binary in a
# directory that others can write (the user-path gate, T-253).
umask 022

# BEGIN PINS (scripts/install-pins.py writes this block; do not edit it)
PODRUNNER_VERSION=""
PODVM_VERSION="0.2.0-beta.39"
PODNET_VERSION="0.4.0-beta.17"
RUNNER_VERSION="2.338.2"
SUM_podrunner_x86_64=""
SUM_podrunner_aarch64=""
SUM_podvm_x86_64="9a774b624d3c21636317d50e3c12723b8bb3206e5ad2c7fdab2ffe5739af94c6"
SUM_podvm_aarch64="ad7164b6e11fb870c0e473fa25785da74989dbed110075c84771d6b09d1639df"
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
UNPACKED_image_x86_64=""
UNPACKED_image_aarch64=""
UNPACKED_imagefull_x86_64=""
UNPACKED_imagefull_aarch64=""
# END PINS

usage() {
    cat <<'EOF'
usage: sh podrunner-install.sh [--prefix DIR] [--full] [--bundle PATH] [--serve]

  --prefix DIR   install podrunner into DIR/bin and the data into
                 DIR/share/podrunner (default: ~/.local, and the data into
                 $XDG_DATA_HOME/podrunner when it is set)
  --full         load the larger runner image (the common tools of
                 ubuntu-latest) and use it for each job
  --bundle PATH  install from a bundle of a release (its .tar.gz or its
                 unpacked directory), with no network
  --serve        install for `podrunner serve`: write no run.toml and no
                 hello.yml; write DATA/serve.json (the paths and the image
                 that `podrunner config init` takes) and print the next step

The environment variable PODRUNNER_MIRROR=URL takes each file from URL/NAME
in place of its release on github.com. Each file must agree with its pin.
A file of the podrunner release must also agree with SHA256SUMS and
release-index.json of the release. With pod-release or cosign on PATH, the
installer also checks the signature of SHA256SUMS.
EOF
}

say() { printf '%s\n' "podrunner-install: $*"; }
die() {
    printf '%s\n' "podrunner-install: error: $*" >&2
    exit 1
}
have() { command -v "$1" > /dev/null 2>&1; }

prefix="" full=0 bundle="" serve=0
while [ $# -gt 0 ]; do
    case $1 in
        --prefix) [ $# -ge 2 ] || die "--prefix needs a directory"; prefix=$2; shift 2 ;;
        --prefix=*) prefix=${1#--prefix=}; shift ;;
        --full) full=1; shift ;;
        --serve) serve=1; shift ;;
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
# podrunner refuses a driver under a directory that the group or the others
# can write (T-537). The umask does not apply to a directory that was there
# before, and a umask of 002 leaves ~/.local/share open: close each
# directory of this user from the binaries up to $HOME, and say so.
me=$(id -u)
close_up() { # DIR
    d=$1
    while [ -n "$d" ] && [ "$d" != "${HOME:-/}" ] && [ "$d" != / ]; do
        if [ -n "$(find "$d" -maxdepth 0 -user "$me" -perm /022)" ]; then
            chmod go-w "$d" || die "cannot remove the write of the group and the others from $d"
            say "ok      $d: the group and the others can no longer write it (a driver under it is refused)"
        fi
        d=$(dirname "$d")
    done
}
close_up "$bin"
close_up "$data/bin"
# A partial file of an earlier run that stopped is never used. Each run
# writes its own (NAME.part.RUN), and a download writes it the whole time,
# so a part file that did not change for an hour is of a run that stopped.
rm -f "$bin/podrunner.part" "$data"/bin/*.part "$downloads"/*.part
find "$bin" "$data/bin" "$downloads" -maxdepth 1 -name '*.part.*' -mmin +60 -exec rm -f {} + 2> /dev/null || true

# The temporary files of this run go at its end, also at a signal (T-249).
# A KILL runs no trap: the next run removes what it left (below).
tmp="" part="" runtmp=""
cleanup() {
    [ -z "$tmp" ] || rm -rf "$tmp"
    [ -z "$part" ] || rm -f "$part"
    rm -rf "$runtmp"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
# The owner of an unpack directory: the boot and the PID namespace of this
# run, then its PID. A PID means a process only in its own namespace.
owner="$(cat /proc/sys/kernel/random/boot_id 2> /dev/null || true) $(readlink /proc/self/ns/pid 2> /dev/null || true)"
# With no boot id or no namespace, a PID tells nothing: only the age holds.
case $owner in " "* | *" ") owner=unknown ;; esac
unpack_stale() { # DIR: true when no live run owns DIR
    m=$1/.podrunner-owner
    if [ -f "$m" ] && [ "$owner" != unknown ] && [ "$(sed -n 1p "$m")" = "$owner" ]; then
        # A PID that is alive a day after its run started is of another
        # process that took the PID again.
        if kill -0 "$(sed -n 2p "$m")" 2> /dev/null && [ -z "$(find "$m" -maxdepth 0 -mmin +1440)" ]; then
            return 1
        fi
        return 0
    fi
    # Another boot or namespace, or no owner file: an unpack takes
    # minutes, so a directory that did not change for an hour is stale.
    [ -f "$m" ] || m=$1
    [ -n "$(find "$m" -maxdepth 0 -mmin +60)" ]
}
for old in "$data"/runner/.unpack.* "$downloads"/run.*; do
    if [ -d "$old" ] && unpack_stale "$old"; then
        rm -rf "$old"
        say "ok      removed $old: a run that stopped left it"
    fi
done
# The files of the release and of the bundle of this run: two installers
# that share a HOME (two containers, where a PID can be the same) each
# remove only their own.
runtmp=$(mktemp -d "$downloads/run.XXXXXXXX") || die "cannot make a directory in $downloads"
printf '%s\n%s\n' "$owner" "$$" > "$runtmp/.podrunner-owner"

if [ -n "$bundle" ] && [ -f "$bundle" ]; then
    # Unpack the archive of a bundle once; a second run unpacks it again.
    mkdir -p "$runtmp/bundle"
    gzip -dc < "$bundle" | tar -xf - -C "$runtmp/bundle" || die "$bundle: not a bundle (.tar.gz)"
    set -- "$runtmp"/bundle/*/PODRUNNER-BUNDLE
    [ -f "$1" ] || die "$bundle: no PODRUNNER-BUNDLE in it: not a bundle of a release"
    bundle=$(dirname "$1")
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

# The signed list of the release (T-523): SHA256SUMS and release-index.json
# of the podrunner release, read once, before the first download of a file
# of that release. A file of the release goes to its place only when
# SHA256SUMS names its pin and the index names its SHA-256. pod-release
# (no network) or cosign checks the signature when one is on PATH.
rel=$runtmp/release
rel_read=0
release_sums() {
    [ "$rel_read" = 0 ] || return 0
    rm -rf "$rel"
    mkdir -p "$rel" || die "cannot make $rel"
    for f in SHA256SUMS release-index.json; do
        fetch "$(url_of podrunner "$f")" "$rel/$f" ||
            die "$f: the download from $(url_of podrunner "$f") failed: the installer takes no file of a release with no $f"
    done
    tag=v$PODRUNNER_VERSION
    identity=https://github.com/PodUtils/podrunner/.github/workflows/release.yml@refs/tags/$tag
    if have pod-release || have cosign; then
        fetch "$(url_of podrunner SHA256SUMS.sigstore.json)" "$rel/SHA256SUMS.sigstore.json" ||
            die "SHA256SUMS.sigstore.json: the download from $(url_of podrunner SHA256SUMS.sigstore.json) failed"
    fi
    if have pod-release; then
        pod-release verify --repo PodUtils/podrunner --tag "$tag" "$rel" >&2 ||
            die "pod-release verify refused SHA256SUMS or release-index.json of $tag (the signer must be $identity)"
        say "ok      the signature of SHA256SUMS and release-index.json ($identity, pod-release)"
    elif have cosign; then
        cosign verify-blob --bundle "$rel/SHA256SUMS.sigstore.json" --certificate-identity "$identity" \
            --certificate-oidc-issuer https://token.actions.githubusercontent.com "$rel/SHA256SUMS" >&2 ||
            die "cosign verify-blob refused SHA256SUMS of $tag (the signer must be $identity; cosign can need the network: put pod-release on PATH, as it needs none)"
        say "ok      the signature of SHA256SUMS ($identity, cosign)"
    else
        say "warn    no pod-release and no cosign on PATH: the signature of SHA256SUMS is not checked (docs/install.md, The checks)"
    fi
    isum=$(listed release-index.json)
    [ -n "$isum" ] || die "release-index.json: SHA256SUMS of $tag has no line of it"
    [ "$(sha256_of "$rel/release-index.json")" = "$isum" ] ||
        die "release-index.json: the SHA-256 is $(sha256_of "$rel/release-index.json"), not $isum of SHA256SUMS"
    grep -qF "\"$tag\"" "$rel/release-index.json" || die "release-index.json: it does not name the tag $tag"
    # This installer is a file of the release: SHA256SUMS names its SHA-256.
    # A copy that a pipe gives (curl ... | sh) has no file to read.
    if [ -f "$0" ]; then
        self=$(sha256_of "$0")
        [ "$(listed podrunner-install.sh)" = "$self" ] ||
            die "$0: the SHA-256 of this installer is $self, not the line of podrunner-install.sh in SHA256SUMS of $tag: take the installer of the release"
        say "ok      this installer is podrunner-install.sh of SHA256SUMS of $tag"
    fi
    rel_read=1
}
# listed NAME: the SHA-256 that SHA256SUMS of the release gives for NAME.
listed() { awk -v n="$1" '$2 == n || $2 == "*" n { print $1 }' "$rel/SHA256SUMS"; }

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
    part="$3.part.${runtmp##*.}"
    rm -f "$part"
    if [ -n "$bundle" ]; then
        [ -f "$bundle/$2" ] || die "$2: not in the bundle $bundle (download it from the release into that directory)"
        cp "$bundle/$2" "$part" || die "$2: cannot copy it from the bundle"
    else
        case $1 in podrunner | image | imagefull) release_sums ;; esac
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
    if [ "$rel_read" = 1 ]; then
        case $1 in
            podrunner | image | imagefull)
                line=$(listed "$2")
                [ "$line" = "$want" ] || {
                    rm -f "$part"
                    die "$2: SHA256SUMS of v$PODRUNNER_VERSION gives ${line:-no line}, not the pin $want: the release and this installer differ"
                }
                grep -qF "\"$want\"" "$rel/release-index.json" || {
                    rm -f "$part"
                    die "$2: release-index.json of v$PODRUNNER_VERSION does not name its SHA-256 $want"
                }
                ;;
        esac
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
    rm -rf "$runner_dir"
    mkdir -p "$data/runner" || die "cannot make $data/runner"
    tmp=$(mktemp -d "$data/runner/.unpack.XXXXXXXX") || die "cannot make a directory in $data/runner"
    # mktemp gives 0700; the runner directory keeps the mode of the umask.
    chmod "$(printf '%o' $((0777 & ~0$(umask))))" "$tmp" || die "cannot set the mode of $tmp"
    printf '%s\n%s\n' "$owner" "$$" > "$tmp/.podrunner-owner"
    gzip -dc < "$downloads/$tgz" | tar -xf - -C "$tmp" || die "$tgz: the unpack failed"
    printf '%s\n' "$runner_sum" > "$tmp/.podrunner-install"
    rm -f "$tmp/.podrunner-owner"
    mv "$tmp" "$runner_dir"
    tmp=""
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
    image=$(pin DIGEST_imagefull) image_bytes=$(pin UNPACKED_imagefull)
else
    load_image image "runner-$arch.tar"
    image=$(pin DIGEST_image) image_bytes=$(pin UNPACKED_image)
fi
rm -rf "$runtmp/bundle" "$rel"

# An install for `serve` (T-507): a program (githome's CLI) reads
# serve.json and runs `podrunner config init`; no file of the local mode.
if [ "$serve" = 1 ]; then
    # The unpacked size of the image (T-572): `check` refuses a pin above
    # the bound of a kind. A release before it has none.
    printf '{"schema": 1, "podrunner": "%s", "podvm": "%s", "podnet": "%s", "images": "%s", "image": "runner@%s", "runner_version": "%s", "unpacked_bytes": %s}\n' \
        "$bin/podrunner" "$podvm" "$data/bin/podnet" "$store" "$image" "$RUNNER_VERSION" "${image_bytes:-null}" > "$data/serve.json.new"
    mv -f "$data/serve.json.new" "$data/serve.json"
    say "ok      $data/serve.json"
    say "the checks of podrunner doctor:"
    "$bin/podrunner" doctor 2>&1 | grep -v '^missing: --config' | sed 's/^/    /' || true
    [ "$got" -eq 0 ] && say "nothing was downloaded: each file was present and correct"
    size=${image_bytes:+ --unpacked-bytes $image_bytes}
    say "next: $bin/podrunner config init --source NAME --program PATH --podvm $podvm --images $store --image runner@$image --built-at-s N$size --out FILE (N: built_at_s of podrunner-manifest.json)"
    exit 0
fi

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
