#!/bin/sh
# Check that the design copy (docs/design/, GitHub #14) holds no detail of
# a machine or an account of the owner: no host name in a runner log, no
# home or root path of a machine, no statement about the state of an
# account. GitHub is public.
#
#   sh scripts/check-design.sh [DIR]      (default docs/design)
#   sh scripts/check-design.sh --self-test
#
# Exit 0 clean, 1 a finding.
set -u
here=$(cd "$(dirname "$0")" && pwd)

scan() {
  dir=$1
  found=0
  # The runner sends the host name in ownerName; a log keeps only "<hostname>".
  grep -rnE '"ownerName":"[^<]' "$dir" && found=1
  # A home or root path of a machine. /home/runner is the user of the image.
  grep -rnE '(^|[^A-Za-z0-9_$.{}/-])/(Users|root)/' "$dir" && found=1
  grep -rnE '/home/' "$dir" | grep -vE '/home/runner\b' && found=1
  # A statement about the state of an account.
  grep -rniE 'account (is|as|was) (suspended|banned|locked|disabled)' "$dir" && found=1
  return $found
}

if [ "${1:-}" = --self-test ]; then
  work=$(mktemp -d)
  trap 'rm -rf "$work"' EXIT
  fails=0
  plant() {
    rm -rf "$work/d"; mkdir -p "$work/d"
    printf '%s\n' "$1" > "$work/d/plant.md"
    if scan "$work/d" > /dev/null; then echo "self-test: not found: $2"; fails=1; fi
  }
  plant 'body={"ownerName":"build-host-1 (PID: 1)"}' "a host name in ownerName"
  plant 'Work in /Users/someone/src.' "a path under /Users"
  plant 'Work in /root/src.' "a path under /root"
  plant 'Work in /home/someone/src.' "a path under /home"
  plant 'The gh account is suspended.' "a statement about an account"
  rm -rf "$work/d"; mkdir -p "$work/d"
  printf '%s\n' 'body={"ownerName":"<hostname> (PID: 1)"}' 'Cmd: /home/runner/run.sh' 'mkdir -p "$S/root/bin"' > "$work/d/clean.md"
  scan "$work/d" > /dev/null || { echo "self-test: a clean file fails"; fails=1; }
  [ "$fails" -eq 0 ] && echo "check-design: self-test passed"
  exit "$fails"
fi

dir=${1:-$here/../docs/design}
if scan "$dir"; then
  echo "check-design: $dir is clean"
else
  echo "check-design: a detail of a machine or an account is in $dir" >&2
  exit 1
fi
