#!/bin/sh
# SPDX-License-Identifier: 0BSD
# The subcommands that the shell scripts of the hosted snapshot give to
# scrub.py must be ones that scrub.py takes: a self-test of scrub.py does
# not run the lines of capture.sh, and a typo there stops a capture at its
# first step.
#
#   sh scripts/hosted/calls.sh [FILE...]
set -eu
here=$(cd "$(dirname "$0")" && pwd)
if [ $# -eq 0 ]; then
    for f in "$here"/*.sh; do
        [ "$f" = "$here/calls.sh" ] || set -- "$@" "$f"
    done
fi
known=$(python3 -I "$here/scrub.py" --help | sed -n 's/^ *{\([a-z,]*\)}.*/\1/p' | head -n 1)
[ -n "$known" ] || { echo "calls: scrub.py --help names no subcommand" >&2; exit 1; }
rc=0
for f in "$@"; do
    words=$(sed -n 's/.*scrub\.py" \([^ ]*\).*/\1/p' "$f")
    while read -r w; do
        [ -n "$w" ] || continue
        case ",$known," in
        *",$w,"*) ;;
        *) echo "calls: $f: scrub.py $w: not a subcommand ($known)" >&2; rc=1 ;;
        esac
    done <<WORDS
$words
WORDS
done
exit "$rc"
