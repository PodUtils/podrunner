#!/bin/sh
# SPDX-License-Identifier: 0BSD
# The test of the installer (T-202) against a local fixture: small fake
# assets with their own pins, served from a directory (file:// of
# PODRUNNER_MIRROR). No network. The cases: the pins of the tree agree
# with images/*.toml, and a changed pin fails; a ghcr.io reference of
# another version fails; a copy with no pins of a
# release refuses; a fresh install; a second run that downloads nothing
# (its mirror is empty); a corrupt asset fails by name and leaves no file;
# a changed binary is replaced; a file of the user stays; the offline
# bundle, as an archive and from its directory; --full. The assets of a
# release from scripts/release-assets.sh; the rule of images_run
# (scripts/images-run.sh); an install from a local HTTP
# server; the man page (T-692: its place, its pin, a second run, a corrupt
# page, a release with no page, the bundle); a lane of the user-path gate (scripts/user-gate.py, T-243) on the
# fake assets, and a lane with a missing asset that fails.
#
#   sh scripts/test-install.sh
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
t=$(mktemp -d)
trap 'rm -rf "$t"' EXIT
pass=0
ok() {
    pass=$((pass + 1))
    echo "ok $pass $*"
}
fail() {
    echo "FAIL $*" >&2
    [ ! -f "$t/out" ] || sed 's/^/  | /' "$t/out" >&2
    exit 1
}
case $(uname -m) in
    x86_64 | amd64) arch=x86_64 rarch=x64 other=aarch64 orarch=arm64 ;;
    aarch64 | arm64) arch=aarch64 rarch=arm64 other=x86_64 orarch=x64 ;;
    *) echo "test-install: skip: no release for $(uname -m)"; exit 0 ;;
esac

# 1. The pins of the tree agree with images/*.toml; a changed pin fails.
python3 -I "$root/scripts/install-pins.py" --check "$root/scripts/install.sh" > "$t/out" || fail "the pins of the tree"
sed 's/^PODVM_VERSION=.*/PODVM_VERSION="0.0.1"/' "$root/scripts/install.sh" > "$t/planted.sh"
if python3 -I "$root/scripts/install-pins.py" --check "$t/planted.sh" > "$t/out"; then fail "a planted pin passed the check"; fi
grep -q 'want PODVM_VERSION=' "$t/out" || fail "the check does not name the pin"
sed 's/^SUM_manpage=.*/SUM_manpage="00"/' "$root/scripts/install.sh" > "$t/planted.sh"
if python3 -I "$root/scripts/install-pins.py" --check "$t/planted.sh" > "$t/out"; then fail "a planted pin of the page passed the check"; fi
grep -q 'want SUM_manpage=""' "$t/out" || fail "the check does not name the pin of the page"
ok "the pins of the tree agree; a changed pin fails"
# T-553: a new version of podvm with the ghcr.io reference of the old one
# fails.
mkdir -p "$t/stale"
cp -R "$root/images/." "$t/stale/"
grep -q '^reference = "ghcr.io/podutils/podvm:v' "$t/stale/podvm.toml" || fail "images/podvm.toml has no [ghcr] reference"
sed 's/^version = ".*"/version = "0.0.1"/' "$root/images/podvm.toml" > "$t/stale/podvm.toml"
if python3 -I "$root/scripts/install-pins.py" --images "$t/stale" --check "$root/scripts/install.sh" > "$t/out" 2>&1; then
    fail "a ghcr.io reference of another version passed"
fi
grep -q 'reference' "$t/out" || fail "the check does not name the reference"
ok "a ghcr.io reference of another version fails"

# The fixture: fake binaries, a runner tarball and image archives.
a=$t/assets
mkdir -p "$a" "$t/images/runner" "$t/log"
cat > "$a/podrunner-$arch-unknown-linux-musl" <<'EOF'
#!/bin/sh
case "$1 ${2:-}" in
    "doctor --run") echo "ok   workflow: $3" ;;
    "doctor ") echo "kvm: no"; echo "probe unshare(CLONE_NEWUSER): ok"; echo "missing: --config: no unit kind" ;;
    "run "*)
        # A job log in --log-dir, as a run writes it (the checks pids:podvm
        # and pty:podvm of the gate read it).
        ld=; prev=
        for x in "$@"; do [ "$prev" != --log-dir ] || ld=$x; prev=$x; done
        [ -z "$ld" ] || { mkdir -p "$ld/job.0" && printf 'host pid: not seen\na pty: open\n' > "$ld/job.0/job.log"; }
        echo "ok      hello" ;;
    *) echo "podrunner 9.9.9-test" ;;
esac
EOF
# The fake podvm keeps one file for each loaded digest, and logs each load.
cat > "$a/podvm-$arch" <<EOF
#!/bin/sh
set -eu
case \$1 in
    load)
        d=\$(tar -xOf "\$3" index.json | sed 's/.*"digest": *"\\(sha256:[0-9a-f]*\\)".*/\\1/')
        mkdir -p "\$PODVM_STORE" && touch "\$PODVM_STORE/\$d" && echo "\$d" >> "$t/log/loads" ;;
    inspect) test -f "\$PODVM_STORE/\${2#runner@}" ;;
    verify) true ;;
    *) echo "podvm 0.0.0-test" ;;
esac
EOF
printf '#!/bin/sh\necho podnet-test\n' > "$a/podnet-$arch-unknown-linux-musl"
# The page of a release (T-645): one file for both architectures.
printf '.TH PODRUNNER 1 "2026-11-26" "podrunner 9.9.9-test" "User Commands"\n.SH NAME\npodrunner \\- a test page\n' > "$a/podrunner.1"
mkdir -p "$t/r/bin"
printf '#!/bin/sh\n' > "$t/r/run.sh"
printf 'listener\n' > "$t/r/bin/Runner.Listener"
tar -cf - -C "$t/r" . | gzip -c > "$a/actions-runner-linux-$rarch-2.0.0.tar.gz"
# Two layers of an image (T-572): the first has 100, 50 and 20 bytes; the
# second removes the 50 bytes with a whiteout and the 20 bytes with an
# opaque directory, and adds 7 and 3; unpacked: 110.
mkdir -p "$t/l1/dir" "$t/l1/opq" "$t/l2/dir" "$t/l2/opq"
head -c 100 /dev/zero > "$t/l1/dir/f1"
head -c 50 /dev/zero > "$t/l1/dir/f2"
head -c 20 /dev/zero > "$t/l1/opq/old"
: > "$t/l2/dir/.wh.f2"
: > "$t/l2/opq/.wh..wh..opq"
head -c 3 /dev/zero > "$t/l2/opq/new"
head -c 7 /dev/zero > "$t/l2/g"
image() { # NAME TEXT
    mkdir -p "$t/oci/$1/blobs/sha256"
    printf '%s\n' "$2" > "$t/oci/$1/blobs/sha256/x"
    d=$(printf '%s' "$2" | sha256sum | cut -d' ' -f1)
    printf '{"schemaVersion": 2, "manifests": [{"digest": "sha256:%s", "annotations": {"org.opencontainers.image.ref.name": "runner:%s"}}]}\n' "$d" "$1" \
        > "$t/oci/$1/index.json"
    # The manifest at the digest of the index, its config with the time of
    # the build (the manifest of a release reads it, T-499), and its layers.
    printf '{"config": {"digest": "sha256:cfg"}, "layers": [{"mediaType": "application/vnd.oci.image.layer.v1.tar+gzip", "digest": "sha256:l1"}, {"mediaType": "application/vnd.oci.image.layer.v1.tar", "digest": "sha256:l2"}]}\n' \
        > "$t/oci/$1/blobs/sha256/$d"
    printf '{"created": "2026-11-01T00:00:00Z"}\n' > "$t/oci/$1/blobs/sha256/cfg"
    tar -cf - -C "$t/l1" . | gzip -c > "$t/oci/$1/blobs/sha256/l1"
    tar -cf "$t/oci/$1/blobs/sha256/l2" -C "$t/l2" .
    (cd "$t/oci/$1" && tar -cf "$a/$1" index.json blobs/sha256/x "blobs/sha256/$d" blobs/sha256/cfg blobs/sha256/l1 blobs/sha256/l2)
}
image "runner-$arch.tar" base
image "runner-full-$arch.tar" full
# The other architecture: a release needs its binary, its image and a
# bundle; the copies have the same pins.
cp "$a/podrunner-$arch-unknown-linux-musl" "$a/podrunner-$other-unknown-linux-musl"
cp "$a/runner-$arch.tar" "$a/runner-$other.tar"
cp "$a/podvm-$arch" "$a/podvm-$other"
cp "$a/podnet-$arch-unknown-linux-musl" "$a/podnet-$other-unknown-linux-musl"
cp "$a/actions-runner-linux-$rarch-2.0.0.tar.gz" "$a/actions-runner-linux-$orarch-2.0.0.tar.gz"
(cd "$a" && for f in runner-*.tar; do sha256sum "$f" > "$f.sha256"; done)
sum() { sha256sum < "$1" | cut -d' ' -f1; }
pins() { # FILE NAME VERSION
    printf 'version = "%s"\n\n[sha256]\n%s = "%s"\n%s = "%s"\n' "$3" "$arch" "$(sum "$a/$2")" "$other" "$(sum "$a/$2")" > "$1"
}
pins "$t/images/podvm.toml" "podvm-$arch" 0.0.0-test
pins "$t/images/podnet.toml" "podnet-$arch-unknown-linux-musl" 0.0.0-test
pins "$t/images/runner/runner.toml" "actions-runner-linux-$rarch-2.0.0.tar.gz" 2.0.0
installer=$t/podrunner-install.sh
python3 -I "$root/scripts/install-pins.py" --images "$t/images" --release 9.9.9-test --assets "$a" \
    "$root/scripts/install.sh" --out "$installer" > /dev/null
mkdir -p "$t/empty"
# index DIR: a release-index.json of each file of DIR, in the form of
# release-index.py of podcrates (no network: the test does not fetch it).
index() {
    python3 -I - "$1" <<'PY'
import hashlib, json, pathlib, sys
d = pathlib.Path(sys.argv[1])
own = ("SHA256SUMS", "SHA256SUMS.sigstore.json", "release-index.json")
files = [{"name": f.name, "sha256": hashlib.sha256(f.read_bytes()).hexdigest(), "size": f.stat().st_size}
         for f in sorted(d.iterdir()) if f.is_file() and f.name not in own and not f.name.endswith(".sha256")]
doc = {"schema": 1, "project": "PodUtils/podrunner", "version": "v9.9.9-test", "date": "2026-11-20T00:00:00Z", "files": files}
(d / "release-index.json").write_text(json.dumps(doc, indent=2) + "\n")
PY
}
# The fixture is a release (T-523): the installer, release-index.json and
# SHA256SUMS beside the assets.
cp "$installer" "$a/podrunner-install.sh"
index "$a"
# sums DIR: SHA256SUMS of DIR and a bundle with no real signature.
sums() {
    sh "$root/scripts/release-assets.sh" sums "$1" > /dev/null
    echo '{"fake": "no signature"}' > "$1/SHA256SUMS.sigstore.json"
}
sums "$a"
# A fake pod-release: it logs its arguments and exits with the code in
# $t/verify-rc (the fixture has no real signature).
mkdir -p "$t/verify"
cat > "$t/verify/pod-release" <<EOF
#!/bin/sh
echo "\$*" >> "$t/verify.log"
exit "\$(cat "$t/verify-rc" 2> /dev/null || echo 0)"
EOF
chmod 0755 "$t/verify/pod-release"

# run NAME [ARGS]: the installer in the home $t/NAME, with the mirror
# $mirror, its output in $t/out.
mirror=file://$a
run() {
    h=$t/$1
    shift
    mkdir -p "$h"
    env -u XDG_DATA_HOME -u XDG_CONFIG_HOME HOME="$h" PATH="${path:-$t/verify:/usr/bin:/bin}" PODRUNNER_MIRROR="$mirror" \
        sh "${script:-$installer}" "$@" > "$t/out" 2>&1
}
loads() { if [ -f "$t/log/loads" ]; then wc -l < "$t/log/loads" | tr -d ' '; else echo 0; fi; }

# 2. A copy of the tree has no pins of a release, and refuses.
if script=$root/scripts/install.sh run tree; then fail "the copy of the tree installed"; fi
grep -q 'no pins of a release' "$t/out" || fail "the copy of the tree does not say why"
ok "a copy with no pins of a release refuses"

# 3. A fresh install.
run home || fail "the fresh install"
d=$t/home/.local/share/podrunner
for f in "$t/home/.local/bin/podrunner" "$d/bin/podvm" "$d/bin/podnet" "$d/runner/2.0.0/run.sh" "$d/hello.yml"; do
    [ -f "$f" ] || fail "the fresh install has no $f"
done
[ -x "$t/home/.local/bin/podrunner" ] || fail "podrunner is not executable"
page=$t/home/.local/share/man/man1/podrunner.1
cmp -s "$page" "$a/podrunner.1" || fail "the page is not at PREFIX/share/man/man1/podrunner.1"
[ "$(stat -c %a "$page")" = 644 ] || fail "the page has the mode $(stat -c %a "$page"), not 644"
grep -q "fetch   podrunner.1" "$t/out" || fail "the page was not fetched"
digest=sha256:$(printf base | sha256sum | cut -d' ' -f1)
[ -f "$d/images/$digest" ] || fail "the image is not in the store"
toml=$t/home/.config/podrunner/run.toml
grep -qx "image = \"runner@$digest\"" "$toml" || fail "run.toml has no image"
grep -qx "podvm = \"$d/bin/podvm\"" "$toml" || fail "run.toml has no podvm"
grep -q 'user namespaces: yes' "$t/out" || fail "no line of the user namespaces"
grep -q 'next, in the tree of your project: .* run .github/workflows/ci.yml --checkout .' "$t/out" || fail "no next command"
[ -z "$(find "$t/home" -name '*.part*')" ] || fail "a partial file stays"
[ "$(loads)" = 1 ] || fail "the image was loaded $(loads) times"
grep -qx "verify --repo PodUtils/podrunner --tag v9.9.9-test $d/downloads/run\.[A-Za-z0-9]*/release" "$t/verify.log" ||
    fail "pod-release verify was not called with the signer of the release"
grep -q 'ok      this installer is podrunner-install.sh of SHA256SUMS of v9.9.9-test' "$t/out" || fail "no check of the installer"
[ -z "$(find "$d/downloads" -mindepth 1 -maxdepth 1 -name 'run.*')" ] || fail "the files of the release stay in downloads"
ok "a fresh install"

# 4. A second run with an empty mirror: it downloads nothing, and writes no
# file again.
page_stat=$(stat -c %i:%Y "$page")
mirror=file://$t/empty
run home || fail "the second run"
grep -q 'nothing was downloaded' "$t/out" || fail "the second run downloaded"
! grep -q '^podrunner-install: fetch' "$t/out" || fail "the second run fetched"
grep -q 'ok      podrunner.1 (present)' "$t/out" || fail "the second run does not say that the page is present"
[ "$(stat -c %i:%Y "$page")" = "$page_stat" ] || fail "the second run wrote the page again"
[ "$(loads)" = 1 ] || fail "the second run loaded the image again"
ok "a second run downloads nothing"

# 4b. A directory that was there before, writable by the group: the
# installer removes that write (a driver in it is refused).
mkdir -p "$t/gw/.local/bin" "$t/gw/.local/share/podrunner/bin"
chmod 0775 "$t/gw" "$t/gw/.local" "$t/gw/.local/bin" "$t/gw/.local/share" "$t/gw/.local/share/podrunner" \
    "$t/gw/.local/share/podrunner/bin"
mirror=file://$a
run gw || fail "the install over a directory that the group can write"
[ -n "$(find "$t/gw" -maxdepth 0 -perm /022)" ] || fail "the installer changed the mode of HOME"
for dir in "$t/gw/.local" "$t/gw/.local/bin" "$t/gw/.local/share" "$t/gw/.local/share/podrunner" \
    "$t/gw/.local/share/podrunner/bin"; do
    [ -z "$(find "$dir" -maxdepth 0 -perm /022)" ] || fail "$dir stays writable by the group or the others"
done
ok "the installer removes the write of the group from its directories"

# 4c. The signed list of the release (T-523). Each refusal exits nonzero,
# names the cause, and puts no file of the release in place.
# refused NAME TEXT WHY: the install in the home NAME fails with TEXT.
refused() {
    if run "$1"; then fail "$3 passed"; fi
    grep -q "$2" "$t/out" || fail "$3: the error does not name the cause"
    [ ! -e "$t/$1/.local/bin/podrunner" ] || fail "$3: podrunner is in place"
}
# release NAME: a copy of the release, served as the mirror.
release() { rm -rf "${t:?}/$1" && cp -R "$a" "$t/$1" && mirror=file://$t/$1; }
bin=podrunner-$arch-unknown-linux-musl
release changed
echo changed >> "$t/changed/$bin"
index "$t/changed" && sums "$t/changed"
refused r-changed "error: $bin: the SHA-256 is .*, not the pin" "a changed asset with a new SHA256SUMS"
release sumline
awk -v n="$bin" '$2 == n { $0 = (substr($0, 1, 1) == "0" ? "1" : "0") substr($0, 2) } { print }' "$a/SHA256SUMS" > "$t/sumline/SHA256SUMS"
refused r-sumline "error: $bin: SHA256SUMS of v9.9.9-test gives .*, not the pin" "a SHA256SUMS that names another SHA-256"
release nosums
rm "$t/nosums/SHA256SUMS"
refused r-nosums "error: SHA256SUMS: the download from .* failed" "a release with no SHA256SUMS"
release noindex
rm "$t/noindex/release-index.json"
refused r-noindex "error: release-index.json: the download from .* failed" "a release with no release-index.json"
release badindex
sed -i "/$(sum "$a/$bin")/d" "$t/badindex/release-index.json"
sums "$t/badindex"
refused r-badindex "error: $bin: release-index.json of v9.9.9-test does not name its SHA-256" "an index with no line of the binary"
# T-692: the page has the same checks as the binary, and no file of the
# release is in place when it fails (the page goes first).
nopage() { [ ! -e "$t/$1/.local/share/man/man1/podrunner.1" ] || fail "$2: the page is in place"; }
release pchanged
echo changed >> "$t/pchanged/podrunner.1"
index "$t/pchanged" && sums "$t/pchanged"
refused r-pchanged "error: podrunner.1: the SHA-256 is .*, not the pin" "a changed page with a new SHA256SUMS"
nopage r-pchanged "a changed page"
release psumline
awk '$2 == "podrunner.1" { $0 = (substr($0, 1, 1) == "0" ? "1" : "0") substr($0, 2) } { print }' "$a/SHA256SUMS" > "$t/psumline/SHA256SUMS"
refused r-psumline "error: podrunner.1: SHA256SUMS of v9.9.9-test gives .*, not the pin" "a SHA256SUMS that names another SHA-256 of the page"
nopage r-psumline "a SHA256SUMS of another page"
release pindex
sed -i "/$(sum "$a/podrunner.1")/d" "$t/pindex/release-index.json"
sums "$t/pindex"
refused r-pindex "error: podrunner.1: release-index.json of v9.9.9-test does not name its SHA-256" "an index with no line of the page"
release pgone
rm "$t/pgone/podrunner.1"
index "$t/pgone" && sums "$t/pgone"
refused r-pgone "error: podrunner.1: the download from .* failed" "a release that lacks the page that its pin names"
nopage r-pgone "a release with no page"
mirror=file://$a
echo 77 > "$t/verify-rc"
refused r-unsigned "error: pod-release verify refused SHA256SUMS" "a signature that pod-release refuses"
rm "$t/verify-rc"
{ cat "$installer" && echo "# changed"; } > "$t/changed-install.sh"
script=$t/changed-install.sh refused r-self "error: .*changed-install.sh: the SHA-256 of this installer is" "a changed installer"
if PATH=/usr/bin:/bin command -v pod-release > /dev/null || PATH=/usr/bin:/bin command -v cosign > /dev/null; then
    echo "test-install: a verifier is in /usr/bin or /bin: the install with no verifier is not run"
else
    path=/usr/bin:/bin run r-nosig || fail "the install with no verifier on PATH"
    grep -q 'warn    no pod-release and no cosign on PATH: the signature of SHA256SUMS is not checked' "$t/out" ||
        fail "the install with no verifier does not say that it checks no signature"
fi
ok "SHA256SUMS and release-index.json of the release, for the binary and the page: a changed asset, a changed SHA256SUMS, no SHA256SUMS, no index, a bad index, a refused signature and a changed installer are refused"

# 5. A changed binary and a partial download are replaced, not used.
mirror=file://$a
echo changed >> "$d/bin/podvm"
echo partial > "$d/bin/podnet.part"
echo partial > "$d/bin/podnet.part.old"
echo partial > "$d/bin/podnet.part.live"
python3 -I -c 'import os, sys, time; t = time.time() - 7200; os.utime(sys.argv[1], (t, t))' "$d/bin/podnet.part.old"
echo changed >> "$page"
run home || fail "the repair"
grep -q "fetch   podrunner.1" "$t/out" || fail "the changed page was not fetched again"
cmp -s "$page" "$a/podrunner.1" || fail "the page was not replaced"
[ ! -f "$d/bin/podnet.part.old" ] || fail "the partial file of an hour ago stays"
[ -f "$d/bin/podnet.part.live" ] || fail "the partial file of a live run is removed"
rm -f "$d/bin/podnet.part.live"
grep -q "fetch   podvm-$arch" "$t/out" || fail "the changed podvm was not fetched again"
[ "$(sum "$d/bin/podvm")" = "$(sum "$a/podvm-$arch")" ] || fail "podvm was not replaced"
[ ! -f "$d/bin/podnet.part" ] || fail "the partial file stays"
ok "a changed binary and a changed page are replaced"

# 6. A corrupt asset fails by name, and leaves no file in its place.
cp -R "$a" "$t/bad"
echo corrupt >> "$t/bad/podvm-$arch"
mirror=file://$t/bad
if run fresh-bad; then fail "a corrupt asset passed"; fi
grep -q "error: podvm-$arch: the SHA-256 is .*, not the pin" "$t/out" || fail "the corrupt asset is not named"
[ ! -e "$t/fresh-bad/.local/share/podrunner/bin/podvm" ] || fail "the corrupt podvm is in place"
[ -z "$(find "$t/fresh-bad" -name '*.part')" ] || fail "a partial file of the corrupt asset stays"
cp -R "$a" "$t/badpage"
echo corrupt >> "$t/badpage/podrunner.1"
mirror=file://$t/badpage
if run fresh-badpage; then fail "a corrupt page passed"; fi
grep -q "error: podrunner.1: the SHA-256 is .*, not the pin" "$t/out" || fail "the corrupt page is not named"
[ -z "$(find "$t/fresh-badpage" -name 'podrunner.1*')" ] || fail "a file of the corrupt page stays"
ok "a corrupt asset fails by name"

# 7. A run.toml of the user stays.
mirror=file://$a
mkdir -p "$t/mine/.config/podrunner"
echo 'podvm = "/my/podvm"' > "$t/mine/.config/podrunner/run.toml"
run mine || fail "the install with a run.toml of the user"
grep -qx 'podvm = "/my/podvm"' "$t/mine/.config/podrunner/run.toml" || fail "the file of the user changed"
[ -f "$t/mine/.config/podrunner/run.toml.podrunner-install" ] || fail "no keys of the install beside it"
ok "a run.toml of the user stays"

# 7a. Each install writes serve.json (T-705): `run --microvm` reads the
# unpacked size of the image from it, with no --serve.
f=$t/mine/.local/share/podrunner/serve.json
[ -f "$f" ] || fail "an install with no --serve wrote no serve.json"
python3 -I -c 'import json,sys; v=json.load(open(sys.argv[1])); assert v["schema"] == 1 and v["image"].startswith("runner@sha256:") and v["unpacked_bytes"] == 110, v' "$f" ||
    fail "serve.json of an install with no --serve has no unpacked size"
[ -f "$t/mine/.config/podrunner/run.toml.podrunner-install" ] || fail "an install with no --serve wrote no run.toml"
ok "serve.json with no --serve"

# 7b. --serve (T-507): serve.json and no file of the local mode; a
# second run changes nothing.
run serve --serve || fail "the install with --serve"
d=$t/serve/.local/share/podrunner
[ -f "$d/serve.json" ] || fail "--serve wrote no serve.json"
python3 -I -c 'import json,sys; v=json.load(open(sys.argv[1])); assert v["image"].startswith("runner@sha256:") and v["podvm"].endswith("/bin/podvm") and v["unpacked_bytes"] == 110, v' "$d/serve.json" ||
    fail "serve.json is not the JSON of the install"
[ ! -e "$t/serve/.config/podrunner/run.toml" ] && [ ! -e "$d/hello.yml" ] || fail "--serve wrote a file of the local mode"
[ ! -e "$t/serve/.config/podrunner" ] || fail "--serve made the directory of the configuration (T-700)"
grep -q 'next: .*podrunner config init --source NAME' "$t/out" || fail "--serve names no next step"
grep -q 'next: .* --built-at-s 1793491200 --unpacked-bytes 110 --out FILE' "$t/out" || fail "--serve names no time of the build and no size of the image"
python3 -I -c 'import json,sys; assert json.load(open(sys.argv[1]))["built_at_s"] == 1793491200' "$d/serve.json" || fail "serve.json has no built_at_s"
before=$(sha256sum < "$d/serve.json")
mirror=file://$t/empty
run serve --serve || fail "the second run with --serve"
[ "$(sha256sum < "$d/serve.json")" = "$before" ] || fail "the second run with --serve changed serve.json"
grep -q 'nothing was downloaded' "$t/out" || fail "the second run with --serve downloaded a file"
mirror=file://$a
ok "--serve"

# 8. The offline bundle: no mirror reaches anything.
mirror=file://$t/empty
sh "$root/scripts/make-bundle.sh" "$installer" "$arch" "$a" "$t/dist" > /dev/null
(cd "$t/dist" && sha256sum -c "podrunner-bundle-$arch.tar.gz.sha256" > /dev/null) || fail "the sum of the bundle"
run offline --bundle "$t/dist/podrunner-bundle-$arch.tar.gz" || fail "the install from the bundle archive"
grep -q 'fetch' "$t/out" && fail "the bundle install fetched"
[ -f "$t/offline/.local/share/podrunner/runner/2.0.0/run.sh" ] || fail "the bundle install has no runner"
[ -z "$(find "$t/offline/.local/share/podrunner/downloads" -mindepth 1 -maxdepth 1 -name 'run.*')" ] || fail "the unpacked bundle stays"
page=$t/offline/.local/share/man/man1/podrunner.1
cmp -s "$page" "$a/podrunner.1" || fail "the bundle install has no page"
ok "the offline bundle, as an archive"

mkdir -p "$t/unpacked"
gzip -dc < "$t/dist/podrunner-bundle-$arch.tar.gz" | tar -xf - -C "$t/unpacked"
b=$t/unpacked/podrunner-bundle-9.9.9-test-$arch
(cd "$b" && sha256sum -c SHA256SUMS > /dev/null) || fail "the SHA256SUMS of the bundle"
script=$b/podrunner-install.sh run offline2 --prefix "$t/offline2/opt" || fail "the install from the bundle directory"
[ -x "$t/offline2/opt/bin/podrunner" ] || fail "--prefix: no podrunner in PREFIX/bin"
[ -f "$t/offline2/opt/share/podrunner/bin/podvm" ] || fail "--prefix: no podvm in PREFIX/share/podrunner"
cmp -s "$t/offline2/opt/share/man/man1/podrunner.1" "$a/podrunner.1" || fail "--prefix: no page in PREFIX/share/man/man1"
cmp -s "$b/podrunner.1" "$a/podrunner.1" || fail "the bundle has no page"
grep -q '  podrunner.1$' "$b/SHA256SUMS" || fail "the SHA256SUMS of the bundle has no line of the page"
if script=$b/podrunner-install.sh run offline3 --full; then fail "--full with no full image in the bundle passed"; fi
grep -q "runner-full-$arch.tar: not in the bundle" "$t/out" || fail "--full does not name the missing image"
cp "$a/runner-full-$arch.tar" "$b/"
script=$b/podrunner-install.sh run offline3 --full || fail "--full from the bundle"
full=sha256:$(printf full | sha256sum | cut -d' ' -f1)
grep -qx "image = \"runner@$full\"" "$t/offline3/.config/podrunner/run.toml" || fail "--full: run.toml has not the full image"
ok "the offline bundle, from its directory, with --prefix and --full"

# 8b. T-692: a bundle of a page that is not its pin fails; a release before
# T-645 has no page and no pin of it: its bundle and its installer skip the
# page, and the install works.
cp -R "$a" "$t/pbad"
echo corrupt >> "$t/pbad/podrunner.1"
if sh "$root/scripts/make-bundle.sh" "$installer" "$arch" "$t/pbad" "$t/pbad-out" > "$t/out" 2>&1; then fail "a bundle with a changed page passed"; fi
grep -q 'podrunner.1: the SHA-256 is .*, not the pin' "$t/out" || fail "the bundle does not name the page"
old=$t/old
mkdir -p "$old"
for f in "$a"/*; do case $f in */podrunner.1 | */podrunner-install.sh | */SHA256SUMS* | */release-index.json) ;; *) cp "$f" "$old/" ;; esac; done
python3 -I "$root/scripts/install-pins.py" --images "$t/images" --release 9.9.9-test --assets "$old" \
    "$root/scripts/install.sh" --out "$old/podrunner-install.sh" > /dev/null
grep -qx 'SUM_manpage=""' "$old/podrunner-install.sh" || fail "the installer of a release with no page has a pin of it"
index "$old"
sums "$old"
mirror=file://$old
script=$old/podrunner-install.sh run old || fail "the install of a release with no page"
grep -qx 'podrunner-install: skip    podrunner.1: this release has no man page' "$t/out" || fail "no line that the page is skipped"
[ -x "$t/old/.local/bin/podrunner" ] || fail "the release with no page has no podrunner"
[ ! -e "$t/old/.local/share/man" ] || fail "the release with no page made a man directory"
sh "$root/scripts/make-bundle.sh" "$old/podrunner-install.sh" "$arch" "$old" "$t/old-dist" > "$t/out" || fail "the bundle of a release with no page"
grep -q 'skip podrunner.1' "$t/out" || fail "the bundle of a release with no page does not say so"
mirror=file://$t/empty
script=$old/podrunner-install.sh run old-offline --bundle "$t/old-dist/podrunner-bundle-$arch.tar.gz" || fail "the install of the bundle with no page"
[ ! -e "$t/old-offline/.local/share/man" ] || fail "the bundle with no page made a man directory"
ok "a release with no page installs and skips the page; a changed page fails the bundle"
mirror=file://$a

# 8c. T-633: the lines that name a command quote their paths (a HOME with a
# space or a quote); a release with no time of the build says N; each cause
# of a failed download has its own line with the fix.
mirror=file://$t/empty
for name in "sp ace" "it's"; do
    run "$name" --bundle "$t/dist/podrunner-bundle-$arch.tar.gz" || fail "the install in a HOME named $name"
    line=$(sed -n 's/^podrunner-install: a first run:  //p' "$t/out")
    eval "set -- $line"
    [ "$#" -eq 3 ] && [ "$1" = "$t/$name/.local/bin/podrunner" ] && [ "$2" = run ] && [ "$3" = "$t/$name/.local/share/podrunner/hello.yml" ] ||
        fail "the line 'a first run' is not one command for the HOME $name: $line"
    line=$(sed -n 's/^podrunner-install: next, in the tree of your project:  //p' "$t/out")
    eval "set -- $line"
    [ "$1" = "$t/$name/.local/bin/podrunner" ] && [ "$2" = run ] || fail "the line 'next' does not quote its program for the HOME $name: $line"
    run "$name-serve" --serve --bundle "$t/dist/podrunner-bundle-$arch.tar.gz" || fail "the install with --serve in a HOME named $name"
    line=$(sed -n 's/^podrunner-install: next: //p' "$t/out" | sed 's/ (--built-at-s is .*//')
    eval "set -- $line"
    [ "$1" = "$t/$name-serve/.local/bin/podrunner" ] && [ "$2" = config ] && [ "$9" = "$t/$name-serve/.local/share/podrunner/bin/podvm" ] || fail "the line 'next' of --serve is not a command for the HOME $name: $line"
done
ok "the lines of the next command quote a path with a space or a quote"
sed 's/^\(BUILT_[a-z]*_[a-z0-9_]*\)=.*/\1=""/' "$installer" > "$t/nobuilt.sh"
script=$t/nobuilt.sh run nobuilt --serve --bundle "$t/dist/podrunner-bundle-$arch.tar.gz" || fail "the install of a release with no time of the build"
grep -q 'next: .* --built-at-s N --unpacked-bytes 110 --out FILE (N: built_at_s of the image in podrunner-manifest.json' "$t/out" ||
    fail "a release with no time of the build does not say where N comes from"
python3 -I -c 'import json,sys; assert json.load(open(sys.argv[1]))["built_at_s"] is None' "$t/nobuilt/.local/share/podrunner/serve.json" ||
    fail "serve.json of a release with no time of the build is not null"
ok "a release with no time of the build says N and writes null"

# A fake curl: it prints the HTTP status of $t/curl-code (as -w does) and
# exits with $t/curl-rc. A fake wget exits with $t/curl-rc.
mkdir -p "$t/fakecurl" "$t/fakewget" "$t/nocurl"
cat > "$t/fakecurl/curl" <<EOT
#!/bin/sh
cat "$t/curl-code" 2> /dev/null
exit "\$(cat "$t/curl-rc")"
EOT
cat > "$t/fakewget/wget" <<EOT
#!/bin/sh
exit "\$(cat "$t/curl-rc")"
EOT
chmod 0755 "$t/fakecurl/curl" "$t/fakewget/wget"
# A PATH with no curl: the other tools of /usr/bin and /bin, by link.
for f in /usr/bin/* /bin/*; do
    case ${f##*/} in curl | wget) ;; *) [ -e "$t/nocurl/${f##*/}" ] || ln -s "$f" "$t/nocurl/${f##*/}" ;; esac
done
# dl NAME TOOL RC CODE TEXT: the install in the home NAME fails, and the
# error has TEXT, the URL, and no secret of a proxy.
dl() {
    echo "$3" > "$t/curl-rc"
    echo "$4" > "$t/curl-code"
    mirror=https://mirror.test:8443/rel
    if [ "$2" = wget ]; then path=$t/fakewget:$t/nocurl; else path=$t/fakecurl:$t/verify:/usr/bin:/bin; fi
    if run "$1"; then path="" fail "the download $1 passed"; fi
    path=""
    grep -q "error: SHA256SUMS: the download from https://mirror.test:8443/rel/SHA256SUMS failed: .*$5" "$t/out" ||
        fail "the download $1 (curl exit $3, HTTP $4): the error is not '$5'"
    ! grep -q 'user:secret' "$t/out" || fail "the download $1 prints the password of the proxy"
    [ ! -e "$t/$1/.local/bin/podrunner" ] || fail "the download $1 left podrunner in place"
}
# A subshell: the proxy variables of the host must not reach the cases.
(
unset HTTPS_PROXY https_proxy HTTP_PROXY http_proxy ALL_PROXY all_proxy NO_PROXY no_proxy
dl d404 curl 22 404 'not found (HTTP 404)'
dl dca curl 77 '' 'TLS check failed (curl exit 77).*CURL_CA_BUNDLE or SSL_CERT_FILE'
dl dca2 curl 60 '' 'TLS check failed (curl exit 60)'
dl dport curl 7 '' 'cannot connect to mirror.test:8443 (curl exit 7)'
dl ddns curl 6 '' 'cannot find the host mirror.test:8443'
dl dtime curl 28 '' 'timed out (curl exit 28)'
dl d405 curl 22 405 'refused (HTTP 405) by the server'
dl d500 curl 22 500 'the server answered HTTP 500'
HTTPS_PROXY=http://user:secret@proxy.test:3128
export HTTPS_PROXY
dl dproxy curl 22 405 'refused (HTTP 405); a proxy is set in the environment.*NO_PROXY=127.0.0.1'
dl dtunnel curl 56 '' 'the connection broke (curl exit 56) with a proxy is set in the environment'
unset HTTPS_PROXY
dl dwca wget 5 '' 'TLS check failed (wget exit 5).*SSL_CERT_FILE'
dl dwnet wget 4 '' 'the network failed (wget exit 4)'
dl dw404 wget 8 '' 'server refused the file or has none (wget exit 8)'
# The real curl: a port with no listener, and a missing file of a mirror.
mirror=http://127.0.0.1:1
if run dead; then fail "the install from a dead port passed"; fi
grep -q 'error: SHA256SUMS: the download from http://127.0.0.1:1/SHA256SUMS failed: cannot connect to 127.0.0.1:1 (curl exit 7)' "$t/out" ||
    fail "a dead port: the error does not name the cause"
mirror=file://$t/nosuch
if run nofile; then fail "the install from a mirror that is not there passed"; fi
grep -q 'failed: no such file: PODRUNNER_MIRROR=file://DIR' "$t/out" || fail "a missing file of a file:// mirror: the error does not name the cause"
)
mirror=file://$a
ok "a failed download names its cause and its fix: a 404, a CA file, a dead port, a name, a timeout, a proxy (curl and wget)"

# 9. The assets of a release, as release.yml and user-gate.yml make them.
RELEASE_ASSETS_IMAGES=$t/images sh "$root/scripts/release-assets.sh" local-mode 9.9.9-test "$a" "$t/rel" > "$t/out" 2>&1 ||
    fail "release-assets.sh local-mode"
for f in podrunner-install.sh podrunner-manifest.json podrunner-bundle-x86_64.tar.gz podrunner-bundle-aarch64.tar.gz "runner-$arch.tar" "runner-full-$arch.tar" tier-cases.json; do
    if [ ! -f "$t/rel/$f" ] || [ ! -f "$t/rel/$f.sha256" ]; then
        fail "release-assets.sh: no $f with its .sha256"
    fi
done
grep -qx 'PODRUNNER_VERSION="9.9.9-test"' "$t/rel/podrunner-install.sh" || fail "release-assets.sh: the installer has not the version"
grep -qx "SUM_manpage=\"$(sum "$a/podrunner.1")\"" "$t/rel/podrunner-install.sh" || fail "release-assets.sh: the installer has not the pin of the page"
# T-484: SHA256SUMS of each asset, with no line of a .sha256 file. The
# binaries and release-index.json go beside them first, as user-gate.yml
# puts them (T-523: the installer needs both).
cp "$a"/podrunner-*-unknown-linux-musl "$a/podrunner.1" "$t/rel/"
index "$t/rel"
sh "$root/scripts/release-assets.sh" sums "$t/rel" > "$t/out" 2>&1 || fail "release-assets.sh sums"
(cd "$t/rel" && sha256sum -c --quiet SHA256SUMS) || fail "SHA256SUMS does not check"
for f in podrunner-install.sh podrunner-manifest.json "runner-$arch.tar" tier-cases.json; do
    grep -q "  $f\$" "$t/rel/SHA256SUMS" || fail "SHA256SUMS has no line of $f"
done
# T-518: the case table of the release is the one of the tree.
cmp -s "$t/rel/tier-cases.json" "$root/crates/podrunner-tier/tier-cases.json" || fail "release-assets.sh: not the tier-cases.json of the tree"
! grep -q '\.sha256$' "$t/rel/SHA256SUMS" || fail "SHA256SUMS lists a .sha256 file"
python3 -I - "$t/rel/podrunner-manifest.json" "$arch" <<'EOF' || fail "release-assets.sh: the manifest"
import json, sys
m = json.load(open(sys.argv[1]))
arch = sys.argv[2]
assert m["schema"] == 1 and m["podrunner"]["version"] == "9.9.9-test", m
assert m["runner"]["version"] and m["podvm"]["version"] and m["podnet"]["version"], m
base = m["runner"]["images"][arch]["base"]
assert base["file"] == f"runner-{arch}.tar" and base["digest"].startswith("sha256:"), base
assert base["built_at_s"] == 1793491200, base
# The files of the layers after the whiteout (T-572).
assert base["unpacked_bytes"] == 110, base
assert m["runner"]["images"][arch]["full"]["unpacked_bytes"] == 110, m
assert len(m["podrunner"]["files"][arch]["sha256"]) == 64, m
# T-692: the page, with the SHA-256 that the installer pins.
page = m["podrunner"]["man_page"]
assert page["file"] == "podrunner.1" and len(page["sha256"]) == 64, page
EOF
ok "the assets of a release (scripts/release-assets.sh)"

# 9b. The rule of the runner-image.yml run of release.yml and user-gate.yml
# (T-626): a green run of an older commit serves only when images/ is the
# same; a red run, another workflow and a changed images/ are refused.
sh "$root/scripts/images-run.sh" --self-test > "$t/out" 2>&1 || fail "the rule of images_run"
ok "the rule of images_run (scripts/images-run.sh)"

# 10. An install from a local HTTP server, through curl.
python3 -I -c 'import functools, http.server, sys
s = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(http.server.SimpleHTTPRequestHandler, directory=sys.argv[1]))
print(s.server_port, flush=True)
s.serve_forever()' "$a" > "$t/port" 2> /dev/null &
server=$!
i=0
while [ ! -s "$t/port" ] && [ "$i" -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
mirror=http://127.0.0.1:$(cat "$t/port")
if run online; then rc=0; else rc=$?; fi
kill "$server"
[ "$rc" -eq 0 ] || fail "the install from a local HTTP server"
grep -q "fetch   podvm-$arch" "$t/out" || fail "the online install fetched no podvm"
ok "an install from a local HTTP server"

# 11. A lane of the user-path gate on the fake assets: the quick start of
# docs/install.md online, the bundle offline, --full, and a run of
# hello.yml with each executor. A lane with no larger image fails.
python3 -I "$root/scripts/user-gate.py" check > "$t/out" 2>&1 || fail "user-gate.py check"
mkdir "$t/host"
# A stub man that finds no page, as on a minimal image: the lane must not
# judge the page with it, whatever man the host has.
mkdir -p "$t/fakeman/none"
printf '#!/bin/sh\nexit 1\n' > "$t/fakeman/none/man"
chmod 0755 "$t/fakeman/none/man"
PATH=$t/fakeman/none:$PATH python3 -I "$root/scripts/user-gate.py" lane --lane local --assets "$t/rel" --out "$t/lane-out" --work "$t/lane" --no-corpus \
    --confine-marker-dir "$t/host" \
    > "$t/out" 2>&1 || fail "a lane of the user-path gate"
python3 -I "$root/scripts/user-gate-summary.py" --lanes local "$t/lane-out" > "$t/out" 2>&1 || fail "the summary of the lane"
# shellcheck disable=SC2016 # the backticks are Markdown.
grep -q '^| `install:full` | pass |$' "$t/out" || fail "the summary has no install:full"
# T-692: the check of the page. A host with no working man judges nothing; a
# man that finds the page with the default manpath passes; a man that finds
# it only with MANPATH fails (the planted defect).
manrow() { awk -F'\t' -v c="$2" '$2 == c { print $3 }' "$1/results.tsv"; }
[ "$(manrow "$t/lane-out" install:man)" = skip ] || fail "the lane on a host with a stub man did not skip install:man"
mkdir -p "$t/fakeman/ok" "$t/fakeman/strict"
cat > "$t/fakeman/ok/man" <<'EOF'
#!/bin/sh
for d in $(echo "${MANPATH:-$HOME/.local/share/man}" | tr : ' '); do
    [ ! -f "$d/man1/$2.1" ] || { echo "$d/man1/$2.1"; exit 0; }
done
exit 1
EOF
# shellcheck disable=SC2016 # the line is the text of a script.
{ echo '#!/bin/sh'; echo '[ -n "${MANPATH:-}" ] || exit 1'; sed 1d "$t/fakeman/ok/man"; } > "$t/fakeman/strict/man"
chmod 0755 "$t/fakeman/ok/man" "$t/fakeman/strict/man"
PATH=$t/fakeman/ok:$PATH python3 -I "$root/scripts/user-gate.py" lane --lane manok --assets "$t/rel" --out "$t/lane-manok" --work "$t/lane-manok-w" --no-corpus \
    --confine-marker-dir "$t/host" > "$t/out" 2>&1 || fail "a lane with a man that finds the page"
[ "$(manrow "$t/lane-manok" install:man)" = pass ] || fail "install:man did not pass with a man that finds the page"
if PATH=$t/fakeman/strict:$PATH python3 -I "$root/scripts/user-gate.py" lane --lane manstrict --assets "$t/rel" --out "$t/lane-manstrict" --work "$t/lane-manstrict-w" --no-corpus \
    --confine-marker-dir "$t/host" > "$t/out" 2>&1; then fail "a lane with a man that needs MANPATH passed"; fi
[ "$(manrow "$t/lane-manstrict" install:man)" = fail ] || fail "install:man did not fail with a man that needs MANPATH"
mkdir -p "$t/rel-bad"
for f in "$t/rel"/*; do case $f in *runner-full-*) ;; *) ln -s "$f" "$t/rel-bad/" ;; esac; done
if python3 -I "$root/scripts/user-gate.py" lane --lane bad --assets "$t/rel-bad" --out "$t/lane-bad" --work "$t/lane" --no-corpus \
    --confine-marker-dir "$t/host" \
    > "$t/out" 2>&1; then fail "a lane with no larger image passed"; fi
if python3 -I "$root/scripts/user-gate-summary.py" --lanes local,bad "$t" > "$t/out" 2>&1; then fail "the summary of a failed lane passed"; fi
# shellcheck disable=SC2016
grep -q '^| `install:full` | pass | \*\*FAIL\*\* |$' "$t/out" || fail "the summary does not show the failed check"
ok "a lane of the user-path gate; a lane with a missing asset fails"

# 11b. A cut run (T-249): INT in the middle of the unpack of the runner
# leaves no `.unpack.*`; KILL leaves one, and the next run removes it. A
# directory of a live run stays; one with no owner file goes after an
# hour. The cut run is the leader of its own process group with INT at
# its default, as a run in a terminal is.
mkdir -p "$t/cut"
printf '#!/bin/sh\n: > "%s/cut/ready"\nexec sleep 60\n' "$t" > "$t/cut/gzip"
chmod 0755 "$t/cut/gzip"
mirror=file://$a
held() { # NAME: start the install in the home $t/NAME; it waits in the unpack
    rm -f "$t/cut/ready"
    mkdir -p "$t/$1"
    env -u XDG_DATA_HOME -u XDG_CONFIG_HOME HOME="$t/$1" PATH="$t/cut:$t/verify:/usr/bin:/bin" PODRUNNER_MIRROR="$mirror" \
        python3 -I -c 'import os, signal, sys; signal.signal(signal.SIGINT, signal.SIG_DFL); os.setsid(); os.execvp("sh", ["sh", sys.argv[1]])' \
        "$installer" > "$t/out-held" 2>&1 &
    pid=$!
    i=0
    while [ ! -f "$t/cut/ready" ] && [ "$i" -lt 300 ]; do sleep 0.1; i=$((i + 1)); done
    [ -f "$t/cut/ready" ] || fail "$1: the install did not reach the unpack"
}
cut() { # NAME SIGNAL: start the install in the home $t/NAME, stop it with SIGNAL in the unpack
    held "$1"
    python3 -I -c 'import os, signal, sys; os.killpg(int(sys.argv[1]), getattr(signal, "SIG" + sys.argv[2]))' "$pid" "$2"
    if wait "$pid"; then fail "$1: the install cut by $2 exited 0"; fi
}
cut cut-int INT
u=$t/cut-int/.local/share/podrunner/runner
[ -z "$(find "$u" -maxdepth 1 -name '.unpack.*')" ] || fail "the install cut by INT left $(find "$u" -maxdepth 1 -name '.unpack.*')"
cut cut-kill KILL
u=$t/cut-kill/.local/share/podrunner/runner
[ -n "$(find "$u" -maxdepth 1 -name '.unpack.*')" ] || fail "the install cut by KILL left no .unpack.* (the case tests nothing)"
# The owner of a live run: the boot, the PID namespace and a live PID.
sleep 60 &
live=$!
mkdir -p "$u/.unpack.live" "$u/.unpack.fresh" "$u/.unpack.old" "$u/.unpack.reused"
printf '%s %s\n%s\n' "$(cat /proc/sys/kernel/random/boot_id)" "$(readlink /proc/self/ns/pid)" "$live" > "$u/.unpack.live/.podrunner-owner"
python3 -I -c 'import os, sys, time; t = time.time() - 7200; os.utime(sys.argv[1], (t, t))' "$u/.unpack.old"
# A live PID in an owner file of two days ago: another process took the PID.
cp "$u/.unpack.live/.podrunner-owner" "$u/.unpack.reused/.podrunner-owner"
python3 -I -c 'import os, sys, time; t = time.time() - 172800; os.utime(sys.argv[1], (t, t))' "$u/.unpack.reused/.podrunner-owner"
run cut-kill || fail "the run after the cut"
kill "$live"
left=$(cd "$u" && find . -maxdepth 1 -name '.unpack.*' | sort | tr '\n' ' ')
[ "$left" = "./.unpack.fresh ./.unpack.live " ] || fail "after the run: $left (want ./.unpack.fresh ./.unpack.live)"
[ -f "$u/2.0.0/run.sh" ] && [ ! -e "$u/2.0.0/.podrunner-owner" ] || fail "the run after the cut has no runner"
grep -q "removed .*/.unpack.old: a run that stopped left it" "$t/out" || fail "the run does not name the removed directory"
grep -q "removed .*/.unpack.reused: a run that stopped left it" "$t/out" || fail "a PID of a day ago keeps its directory"
ok "a cut run: INT leaves no .unpack.*; the next run removes the directory of a KILL"

# 11c. Two installers in one HOME (two containers): the end of the second
# removes none of the files of the first, which waits in its unpack.
held two
dl=$t/two/.local/share/podrunner/downloads
[ -n "$(find "$dl" -name SHA256SUMS)" ] || fail "the first installer has no files of the release (the case tests nothing)"
run two || fail "a second installer beside a live one"
[ -n "$(find "$dl" -name SHA256SUMS)" ] || fail "the end of the second installer removed the files of the release of the first"
python3 -I -c 'import os, signal, sys; os.killpg(int(sys.argv[1]), signal.SIGINT)' "$pid"
if wait "$pid"; then fail "the first installer cut by INT exited 0"; fi
[ -z "$(find "$dl" -mindepth 1 -maxdepth 1 -name 'run.*')" ] || fail "the first installer left $(find "$dl" -mindepth 1 -maxdepth 1 -name 'run.*')"
ok "two installers in one HOME each remove only their own files"

# 12. The scripts pass shellcheck of the pinned version (T-251): the same
# version in CI and on the host of an agent, never skipped.
sh "$root/scripts/shellcheck.sh" "$root/scripts/install.sh" "$root/scripts/make-bundle.sh" \
    "$root/scripts/test-install.sh" "$root/scripts/release-assets.sh" "$root/scripts/shellcheck.sh" \
    "$root/scripts/githome-conformance.sh" "$root/scripts/images-run.sh" \
    "$root"/scripts/lab/*.sh || fail "shellcheck"
ok "shellcheck"
echo "test-install: $pass passed"
