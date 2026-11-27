#!/bin/sh
# SPDX-License-Identifier: 0BSD
# Self-test of scripts/apt-ubuntu.sh with a fake apt-get on PATH, and a
# check that no workflow or script calls apt-get on its own.
#
#   sh scripts/apt-ubuntu-test.sh
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
fail() { echo "FAIL: $*" >&2; exit 1; }

mkdir -p "$work/bin" "$work/etc/sources.list.d"
# The fake records its arguments and the files of the source parts directory.
cat > "$work/bin/apt-get" <<'FAKE'
#!/bin/sh
echo "$*" >> "$FAKE_LOG"
for a in "$@"; do
    case $a in Dir::Etc::sourceparts=*) ls "${a#*=}" >> "$FAKE_LOG" ;; esac
done
FAKE
chmod +x "$work/bin/apt-get"
printf '# no sources\n' > "$work/etc/sources.list"
printf 'Types: deb\nURIs: http://archive.ubuntu.com/ubuntu\n' > "$work/etc/sources.list.d/ubuntu.sources"
printf 'deb https://packages.microsoft.com/ubuntu/24.04/prod noble main\n' > "$work/etc/sources.list.d/microsoft-prod.list"
printf 'Types: deb\nURIs: https://pkgs.zabbly.com/incus/lts-6.0\n' > "$work/zabbly-incus.sources"

run() { PATH="$work/bin:$PATH" FAKE_LOG="$work/log" APT_UBUNTU_ETC="$work/etc" APT_UBUNTU_SUDO='' sh "$root/scripts/apt-ubuntu.sh" "$@"; }

: > "$work/log"
run update
grep -q "^-qq -o Dir::Etc::sourceparts=.* -o Dir::Etc::sourcelist=$work/etc/sources.list update$" "$work/log" || fail "update arguments: $(cat "$work/log")"
grep -qx ubuntu.sources "$work/log" || fail "Ubuntu source is not in the parts directory"
grep -q microsoft "$work/log" && fail "third-party source is in the parts directory"
echo "ok update names only the Ubuntu sources"

: > "$work/log"
run install incus dnsmasq-base
grep -q -- "-y --no-install-recommends install incus dnsmasq-base$" "$work/log" || fail "install arguments: $(cat "$work/log")"
grep -q microsoft "$work/log" && fail "install keeps a third-party source"
echo "ok install names only the Ubuntu sources"

: > "$work/log"
run --keep "$work/zabbly-incus.sources" update
grep -q "zabbly-incus.sources" "$work/log" || fail "kept source is not in the parts directory"
grep -qx ubuntu.sources "$work/log" || fail "Ubuntu source lost with --keep"
grep -q microsoft "$work/log" && fail "--keep let a third-party source in"
echo "ok --keep adds the source of the step"

run --keep "$work/missing.list" update 2> /dev/null && fail "--keep accepted a missing file"
printf x > "$work/notasource.txt"
run --keep "$work/notasource.txt" update 2> /dev/null && fail "--keep accepted a file that is not .list or .sources"
run upgrade 2> /dev/null && fail "a verb other than update and install passed"
run 2> /dev/null && fail "no verb passed"
echo "ok bad input stops"

# One-line Ubuntu list (no deb822 file) works; no Ubuntu source stops.
rm "$work/etc/sources.list.d/ubuntu.sources"
run update 2> /dev/null && fail "no Ubuntu source passed"
printf 'deb http://archive.ubuntu.com/ubuntu noble main\n' > "$work/etc/sources.list"
run update || fail "one-line Ubuntu list failed"
echo "ok one-line style"

# Planted defect: each apt-get call in the workflows and scripts goes through
# scripts/apt-ubuntu.sh.
cd "$root"
bad=$(grep -rnE '(^|[^A-Za-z-])apt(-get)? +(-[A-Za-z-]+ +)*(update|install)' .github/workflows scripts \
    | grep -v '^scripts/apt-ubuntu' || true)
[ -z "$bad" ] || fail "apt called without scripts/apt-ubuntu.sh: $bad"
echo "ok no direct apt-get call"
