#!/bin/sh
# Shared helpers for the m2-podvm measurement scripts. Sourced, not run.
# Resolves everything from this file's directory and from $WORK.
HERE=$(cd "$(dirname "$0")" && pwd)
OUT=$(cd "$HERE/.." && pwd)
WORK=${WORK:-${TMPDIR:-/tmp}/podrunner-m}
mkdir -p "$WORK"
SRC=$WORK/podvm-src
PODVM=${PODVM:-$SRC/target/x86_64-unknown-linux-musl/release/podvm}
# A private store keeps the cold timings honest and leaves the user's store alone.
export XDG_DATA_HOME=${PODVM_XDG:-$WORK/xdg}
mkdir -p "$XDG_DATA_HOME"
# Lines podvm prints on every run that hide the payload's own stderr.
NOISE='Extracted|carry an id|restore ownership|^  the id|^  uid 0|^  extract w|^podvm: (complete|interpose|userland)|^podvm 0|^probes:|^ownership:|^        clone|^this mode|^note:|^entering without|^Digest|^Status|Pulling from|Pull complete'
need() { [ -x "$PODVM" ] || { echo "no podvm binary at $PODVM: run 00-build.sh first" >&2; exit 2; }; }
# run_case NAME CMD... : run, record exit read directly, print stdout head and filtered stderr.
run_case() {
  _n=$1; shift
  _s=$(date +%s.%N)
  "$@" >"$WORK/o.txt" 2>"$WORK/e.txt"
  _rc=$?
  _e=$(date +%s.%N)
  printf '%s exit=%s wall=%.2fs\n' "$_n" "$_rc" "$(awk "BEGIN{print $_e - $_s}")"
  sed 's/^/  stdout: /' "$WORK/o.txt" | head -c 400
  grep -v -E "$NOISE" "$WORK/e.txt" | head -5 | cut -c1-400 | sed 's/^/  stderr: /'
  LAST_RC=$_rc
}
# The loader-directory symlink diagnostic (see 03): link every runner/bin file next to ld.so so the .NET apphost,
# which asks libc realpath(/proc/self/exe) and gets the loader's path, finds its dll. Always undone with hack_off.
hack_on() { sudo -n sh -c 'cd "$1/usr/lib/x86_64-linux-gnu" && for f in "$2"/bin/*; do b=$(basename "$f"); [ -e "$b" ] || [ -L "$b" ] || ln -s "../../../opt/runner/bin/$b" "$b"; done' sh "$1" "$2"; }
hack_off() { sudo -n sh -c 'cd "$1/usr/lib/x86_64-linux-gnu" && for l in *; do [ -L "$l" ] && case "$(readlink "$l")" in ../../../opt/runner/bin/*) rm -f "$l";; esac; done' sh "$1"; }
