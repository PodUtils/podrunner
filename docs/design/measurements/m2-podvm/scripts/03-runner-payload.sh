#!/bin/bash
# Q3 (A-V-04): can podvm run the official actions/runner (linux-x64, v2.338.2) on this host's rung (userland)?
# Pinned input: actions-runner-linux-x64-2.338.2.tar.gz, 228071396 bytes, sha256
#   baebf204307bac6193480852732f0aba2ae97b6c5597c244f4d5558390147c40 (release API of the forge mirror, GitHub asset URL below).
# Image: debian:stable-slim + apt libssl3t64 zlib1g libkrb5-3 libicu76 ca-certificates, prepared by a normal extraction and a
# sudo chroot (needs sudo and the network), then `podvm import`. Cases, each timed 3 times:
#   natural : runner at /opt/runner inside the image, unmodified
#   volume  : runner mounted with -v into the deps image
#   hack    : image + relative symlinks of every runner/bin file in the loader's directory (diagnostic, not a fix)
#   unsafe  : hack image + --unsafe-host-paths with the runner also installed at /opt/runner on the host (sudo; removed after)
# Exit: 0 every case ran and Runner.Listener --version exited 0 on the default rung with the unmodified image; 1 it did not;
# 2 could not prepare (no sudo, no network, checksum mismatch).
. "$(dirname "$0")/lib.sh"; need
URL=https://github.com/actions/runner/releases/download/v2.338.2/actions-runner-linux-x64-2.338.2.tar.gz
SHA=baebf204307bac6193480852732f0aba2ae97b6c5597c244f4d5558390147c40
echo "date=$(date -u +%FT%TZ) kernel=$(uname -r) cpus=$(nproc) virt=$(systemd-detect-virt) podvm=$("$PODVM" --version)"
sudo -n true 2>/dev/null || { echo "no sudo: cannot prepare the rootfs"; exit 2; }
[ -s "$WORK/runner.tgz" ] || curl -qfsSL -m 900 -o "$WORK/runner.tgz" "$URL" || { echo "download failed"; exit 2; }
echo "$SHA  $WORK/runner.tgz" | sha256sum -c - || exit 2
[ -d "$WORK/runner/bin" ] || { rm -rf "$WORK/runner"; mkdir "$WORK/runner" && tar -xzf "$WORK/runner.tgz" -C "$WORK/runner"; }
rm -f "$WORK/runner.tgz.keep"; 
# --- derived rootfs
D=$WORK/drootfs
if [ ! -d "$D/usr" ]; then
  "$PODVM" pull debian:stable-slim >/dev/null 2>&1 || exit 2
  rm -rf "$WORK/oci"; mkdir "$WORK/oci"
  "$PODVM" save -o "$WORK/deb.tar" debian:stable-slim >/dev/null 2>&1 || exit 2
  tar -xf "$WORK/deb.tar" -C "$WORK/oci"; rm -f "$WORK/deb.tar"
  LAYER=$(python3 - "$WORK/oci" <<'P'
import json,sys,os
r=sys.argv[1]
idx=json.load(open(r+"/index.json"));m=json.load(open(r+"/blobs/"+idx["manifests"][0]["digest"].replace(":","/")))
print(r+"/blobs/"+m["layers"][0]["digest"].replace(":","/"))
P
)
  sudo -n mkdir "$D" && sudo -n tar -xzf "$LAYER" -C "$D" || exit 2
  sudo -n cp /etc/resolv.conf "$D/etc/resolv.conf"
  sudo -n mount --bind /proc "$D/proc" 2>/dev/null
  sudo -n chroot "$D" /usr/bin/env http_proxy="$http_proxy" https_proxy="$https_proxy" DEBIAN_FRONTEND=noninteractive \
    sh -c 'apt-get update -qq && apt-get install -y -qq --no-install-recommends libssl3t64 zlib1g libkrb5-3 libicu76 ca-certificates && apt-get clean' >"$WORK/apt.log" 2>&1
  APT=$?; sudo -n umount "$D/proc" 2>/dev/null
  echo "apt exit=$APT"; [ $APT = 0 ] || { tail -5 "$WORK/apt.log"; exit 2; }
  sudo -n rm -rf "$D/var/lib/apt/lists"/* "$D/etc/resolv.conf"; rm -rf "$WORK/oci"
fi
imp() { # imp TAG : tar the rootfs, import, delete the tar
  sudo -n tar -cf "$WORK/img.tar" -C "$D" . && sudo -n chown "$(id -u)" "$WORK/img.tar" && "$PODVM" rmi "$1" >/dev/null 2>&1
  "$PODVM" import "$WORK/img.tar" "$1" 2>&1 | tail -1; rm -f "$WORK/img.tar"; }
timed() { # timed NAME CMD... : 3 runs; prints exit and wall per run, then the first stderr line that is not podvm chatter
  local n=$1; shift; local codes="" walls=""
  for i in 1 2 3; do
    local s e; s=$(date +%s.%N); "$@" >"$WORK/o.txt" 2>"$WORK/e.txt"; local rc=$?; e=$(date +%s.%N)
    codes="$codes $rc"; walls="$walls $(awk "BEGIN{printf \"%.1f\", $e-$s}")"
  done
  LAST_RC=$rc
  echo "$n: exit=[$codes ] wall_s=[$walls ] (3 runs)"
  echo "   stdout: $(head -c 120 "$WORK/o.txt" | head -2 | tr '\n' '|')"
  echo "   first error line: $(grep -v -E "$NOISE" "$WORK/e.txt" | head -1 | cut -c1-300)"
}
R="$PODVM run --rm -q"
FAIL=0
echo "## natural: runner inside the image, unmodified"
hack_off "$D"
sudo -n mkdir -p "$D/opt/runner" && sudo -n cp -a "$WORK/runner/." "$D/opt/runner/"
imp podrunner-full:t
timed natural-version $R podrunner-full:t /opt/runner/bin/Runner.Listener --version; [ $LAST_RC = 0 ] || FAIL=1
timed natural-config-help $R podrunner-full:t sh -c 'cd /opt/runner && ./config.sh --help'
timed natural-version-W0 $R -e DOTNET_EnableWriteXorExecute=0 podrunner-full:t /opt/runner/bin/Runner.Listener --version
timed natural-relative-path-with-workdir $R -w /opt/runner podrunner-full:t ./bin/Runner.Listener --version
echo "## control: what does the apphost see as its own path? (libc readlink is interposed; libc realpath is not)"
timed ctl-readlink-proc-self-exe $R podrunner-full:t readlink /proc/self/exe
timed ctl-realpath-proc-self-exe $R podrunner-full:t realpath /proc/self/exe
"$PODVM" rmi podrunner-full:t >/dev/null 2>&1
echo "## volume: deps image + -v (Degraded on the loader family)"
sudo -n rm -rf "$D/opt/runner"; imp podrunner-deps:t
V="-v $WORK/runner:/opt/runner"
timed volume-direct-first-program $R $V podrunner-deps:t /opt/runner/bin/Runner.Listener --version
timed volume-sh-version $R $V podrunner-deps:t sh -c '/opt/runner/bin/Runner.Listener --version'
timed volume-sh-config-help $R $V podrunner-deps:t sh -c 'cd /opt/runner && ./config.sh --help'
timed volume-workdir-inside $R $V -w /opt/runner podrunner-deps:t sh -c 'ls'
timed volume-ro $R -v "$WORK/runner:/opt/runner:ro" podrunner-deps:t true
timed volume-ls $R $V podrunner-deps:t sh -c 'ls /opt/runner | head -3'
PODVM_MODE=memfd timed volume-memfd $R $V podrunner-deps:t true
"$PODVM" rmi podrunner-deps:t >/dev/null 2>&1
echo "## hack: loader-directory symlinks so the apphost finds its dll (diagnostic)"
sudo -n mkdir -p "$D/opt/runner" && sudo -n cp -a "$WORK/runner/." "$D/opt/runner/"
hack_on "$D" "$WORK/runner"
imp podrunner-hack:t
timed hack-version $R podrunner-hack:t /opt/runner/bin/Runner.Listener --version
timed hack-version-W0 $R -e DOTNET_EnableWriteXorExecute=0 podrunner-hack:t /opt/runner/bin/Runner.Listener --version
timed hack-config-help $R podrunner-hack:t sh -c 'cd /opt/runner && ./config.sh --help'
echo "## hack: where does CoreCLR init fail? (strace of the whole process tree, openat of libcoreclr)"
if command -v strace >/dev/null; then
  strace -f -e trace=openat -o "$WORK/st.txt" $R podrunner-hack:t /opt/runner/bin/Runner.Listener --version >/dev/null 2>&1; echo "strace run exit=$?"
  grep -E 'libcoreclr(traceptprovider)?\.so"' "$WORK/st.txt" | grep -v 'O_WRONLY' | sed -E 's#^[0-9]+ ##; s#sha256_[0-9a-f]{12}[0-9a-f]*#sha256_<digest>#g;' | cut -c1-260 | head -6
else echo "no strace"; fi
echo "## unsafe: hack image + --unsafe-host-paths + runner copied to the host's /opt/runner (disposable host only)"
if sudo -n mkdir /opt/runner 2>/dev/null; then
  sudo -n cp -a "$WORK/runner/." /opt/runner/ && sudo -n chown -R "$(id -u)" /opt/runner
  U="$R --unsafe-host-paths"
  timed unsafe-version $U podrunner-hack:t /opt/runner/bin/Runner.Listener --version
  timed unsafe-version-W0 $U -e DOTNET_EnableWriteXorExecute=0 podrunner-hack:t /opt/runner/bin/Runner.Listener --version
  timed unsafe-config-help-W0 $U -e DOTNET_EnableWriteXorExecute=0 podrunner-hack:t sh -c 'cd /opt/runner && ./config.sh --help'
  sudo -n rm -rf /opt/runner
else echo "/opt/runner exists or cannot be created: unsafe cases skipped"; fi
"$PODVM" rmi podrunner-hack:t >/dev/null 2>&1
hack_off "$D"; sudo -n rm -rf "$D/opt/runner"
exit $FAIL
