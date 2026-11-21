#!/bin/sh
# SPDX-License-Identifier: 0BSD
# The test of the installer (T-202) against a local fixture: small fake
# assets with their own pins, served from a directory (file:// of
# PODRUNNER_MIRROR). No network. The cases: the pins of the tree agree
# with images/*.toml, and a changed pin fails; a copy with no pins of a
# release refuses; a fresh install; a second run that downloads nothing
# (its mirror is empty); a corrupt asset fails by name and leaves no file;
# a changed binary is replaced; a file of the user stays; the offline
# bundle, as an archive and from its directory; --full. The assets of a
# release from scripts/release-assets.sh; an install from a local HTTP
# server; a lane of the user-path gate (scripts/user-gate.py, T-243) on the
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
ok "the pins of the tree agree; a changed pin fails"

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
digest=sha256:$(printf base | sha256sum | cut -d' ' -f1)
[ -f "$d/images/$digest" ] || fail "the image is not in the store"
toml=$t/home/.config/podrunner/run.toml
grep -qx "image = \"runner@$digest\"" "$toml" || fail "run.toml has no image"
grep -qx "podvm = \"$d/bin/podvm\"" "$toml" || fail "run.toml has no podvm"
grep -q 'user namespaces: yes' "$t/out" || fail "no line of the user namespaces"
grep -q 'next, in the tree of your project: .* run .github/workflows/ci.yml --checkout .' "$t/out" || fail "no next command"
[ -z "$(find "$t/home" -name '*.part')" ] || fail "a partial file stays"
[ "$(loads)" = 1 ] || fail "the image was loaded $(loads) times"
grep -qx "verify --repo PodUtils/podrunner --tag v9.9.9-test $d/downloads/release" "$t/verify.log" ||
    fail "pod-release verify was not called with the signer of the release"
grep -q 'ok      this installer is podrunner-install.sh of SHA256SUMS of v9.9.9-test' "$t/out" || fail "no check of the installer"
[ ! -e "$d/downloads/release" ] || fail "the files of the release stay in downloads"
ok "a fresh install"

# 4. A second run with an empty mirror: it downloads nothing.
mirror=file://$t/empty
run home || fail "the second run"
grep -q 'nothing was downloaded' "$t/out" || fail "the second run downloaded"
! grep -q '^podrunner-install: fetch' "$t/out" || fail "the second run fetched"
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
ok "SHA256SUMS and release-index.json of the release: a changed asset, a changed SHA256SUMS, no SHA256SUMS, no index, a bad index, a refused signature and a changed installer are refused"

# 5. A changed binary and a partial download are replaced, not used.
mirror=file://$a
echo changed >> "$d/bin/podvm"
echo partial > "$d/bin/podnet.part"
run home || fail "the repair"
grep -q "fetch   podvm-$arch" "$t/out" || fail "the changed podvm was not fetched again"
[ "$(sum "$d/bin/podvm")" = "$(sum "$a/podvm-$arch")" ] || fail "podvm was not replaced"
[ ! -f "$d/bin/podnet.part" ] || fail "the partial file stays"
ok "a changed binary is replaced"

# 6. A corrupt asset fails by name, and leaves no file in its place.
cp -R "$a" "$t/bad"
echo corrupt >> "$t/bad/podvm-$arch"
mirror=file://$t/bad
if run fresh-bad; then fail "a corrupt asset passed"; fi
grep -q "error: podvm-$arch: the SHA-256 is .*, not the pin" "$t/out" || fail "the corrupt asset is not named"
[ ! -e "$t/fresh-bad/.local/share/podrunner/bin/podvm" ] || fail "the corrupt podvm is in place"
[ -z "$(find "$t/fresh-bad" -name '*.part')" ] || fail "a partial file of the corrupt asset stays"
ok "a corrupt asset fails by name"

# 7. A run.toml of the user stays.
mirror=file://$a
mkdir -p "$t/mine/.config/podrunner"
echo 'podvm = "/my/podvm"' > "$t/mine/.config/podrunner/run.toml"
run mine || fail "the install with a run.toml of the user"
grep -qx 'podvm = "/my/podvm"' "$t/mine/.config/podrunner/run.toml" || fail "the file of the user changed"
[ -f "$t/mine/.config/podrunner/run.toml.podrunner-install" ] || fail "no keys of the install beside it"
ok "a run.toml of the user stays"

# 7b. --serve (T-507): serve.json and no file of the local mode; a
# second run changes nothing.
run serve --serve || fail "the install with --serve"
d=$t/serve/.local/share/podrunner
[ -f "$d/serve.json" ] || fail "--serve wrote no serve.json"
python3 -I -c 'import json,sys; v=json.load(open(sys.argv[1])); assert v["image"].startswith("runner@sha256:") and v["podvm"].endswith("/bin/podvm") and v["unpacked_bytes"] == 110, v' "$d/serve.json" ||
    fail "serve.json is not the JSON of the install"
[ ! -e "$t/serve/.config/podrunner/run.toml" ] && [ ! -e "$d/hello.yml" ] || fail "--serve wrote a file of the local mode"
grep -q 'next: .*podrunner config init --source NAME' "$t/out" || fail "--serve names no next step"
grep -q 'next: .* --built-at-s N --unpacked-bytes 110 --out FILE' "$t/out" || fail "--serve names no size of the image"
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
[ ! -d "$t/offline/.local/share/podrunner/downloads/bundle" ] || fail "the unpacked bundle stays"
ok "the offline bundle, as an archive"

mkdir -p "$t/unpacked"
gzip -dc < "$t/dist/podrunner-bundle-$arch.tar.gz" | tar -xf - -C "$t/unpacked"
b=$t/unpacked/podrunner-bundle-9.9.9-test-$arch
(cd "$b" && sha256sum -c SHA256SUMS > /dev/null) || fail "the SHA256SUMS of the bundle"
script=$b/podrunner-install.sh run offline2 --prefix "$t/offline2/opt" || fail "the install from the bundle directory"
[ -x "$t/offline2/opt/bin/podrunner" ] || fail "--prefix: no podrunner in PREFIX/bin"
[ -f "$t/offline2/opt/share/podrunner/bin/podvm" ] || fail "--prefix: no podvm in PREFIX/share/podrunner"
if script=$b/podrunner-install.sh run offline3 --full; then fail "--full with no full image in the bundle passed"; fi
grep -q "runner-full-$arch.tar: not in the bundle" "$t/out" || fail "--full does not name the missing image"
cp "$a/runner-full-$arch.tar" "$b/"
script=$b/podrunner-install.sh run offline3 --full || fail "--full from the bundle"
full=sha256:$(printf full | sha256sum | cut -d' ' -f1)
grep -qx "image = \"runner@$full\"" "$t/offline3/.config/podrunner/run.toml" || fail "--full: run.toml has not the full image"
ok "the offline bundle, from its directory, with --prefix and --full"

# 9. The assets of a release, as release.yml and user-gate.yml make them.
RELEASE_ASSETS_IMAGES=$t/images sh "$root/scripts/release-assets.sh" local-mode 9.9.9-test "$a" "$t/rel" > "$t/out" 2>&1 ||
    fail "release-assets.sh local-mode"
for f in podrunner-install.sh podrunner-manifest.json podrunner-bundle-x86_64.tar.gz podrunner-bundle-aarch64.tar.gz "runner-$arch.tar" "runner-full-$arch.tar"; do
    if [ ! -f "$t/rel/$f" ] || [ ! -f "$t/rel/$f.sha256" ]; then
        fail "release-assets.sh: no $f with its .sha256"
    fi
done
grep -qx 'PODRUNNER_VERSION="9.9.9-test"' "$t/rel/podrunner-install.sh" || fail "release-assets.sh: the installer has not the version"
# T-484: SHA256SUMS of each asset, with no line of a .sha256 file. The
# binaries and release-index.json go beside them first, as user-gate.yml
# puts them (T-523: the installer needs both).
cp "$a"/podrunner-*-unknown-linux-musl "$t/rel/"
index "$t/rel"
sh "$root/scripts/release-assets.sh" sums "$t/rel" > "$t/out" 2>&1 || fail "release-assets.sh sums"
(cd "$t/rel" && sha256sum -c --quiet SHA256SUMS) || fail "SHA256SUMS does not check"
for f in podrunner-install.sh podrunner-manifest.json "runner-$arch.tar"; do
    grep -q "  $f\$" "$t/rel/SHA256SUMS" || fail "SHA256SUMS has no line of $f"
done
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
EOF
ok "the assets of a release (scripts/release-assets.sh)"

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
python3 -I "$root/scripts/user-gate.py" lane --lane local --assets "$t/rel" --out "$t/lane-out" --work "$t/lane" --no-corpus \
    > "$t/out" 2>&1 || fail "a lane of the user-path gate"
python3 -I "$root/scripts/user-gate-summary.py" --lanes local "$t/lane-out" > "$t/out" 2>&1 || fail "the summary of the lane"
# shellcheck disable=SC2016 # the backticks are Markdown.
grep -q '^| `install:full` | pass |$' "$t/out" || fail "the summary has no install:full"
mkdir -p "$t/rel-bad"
for f in "$t/rel"/*; do case $f in *runner-full-*) ;; *) ln -s "$f" "$t/rel-bad/" ;; esac; done
if python3 -I "$root/scripts/user-gate.py" lane --lane bad --assets "$t/rel-bad" --out "$t/lane-bad" --work "$t/lane" --no-corpus \
    > "$t/out" 2>&1; then fail "a lane with no larger image passed"; fi
if python3 -I "$root/scripts/user-gate-summary.py" --lanes local,bad "$t" > "$t/out" 2>&1; then fail "the summary of a failed lane passed"; fi
# shellcheck disable=SC2016
grep -q '^| `install:full` | pass | \*\*FAIL\*\* |$' "$t/out" || fail "the summary does not show the failed check"
ok "a lane of the user-path gate; a lane with a missing asset fails"

# 12. The scripts pass shellcheck of the pinned version (T-251): the same
# version in CI and on the host of an agent, never skipped.
sh "$root/scripts/shellcheck.sh" "$root/scripts/install.sh" "$root/scripts/make-bundle.sh" \
    "$root/scripts/test-install.sh" "$root/scripts/release-assets.sh" "$root/scripts/shellcheck.sh" \
    "$root/scripts/githome-conformance.sh" \
    "$root"/scripts/lab/*.sh || fail "shellcheck"
ok "shellcheck"
echo "test-install: $pass passed"
