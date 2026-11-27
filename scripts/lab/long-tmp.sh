#!/bin/sh
# SPDX-License-Identifier: 0BSD
# The class long-tmp (T-735): TMPDIR is a directory whose path has
# LAB_TMP_LEN bytes (default 100). sun_path holds a Unix socket path of
# 107 bytes, so a socket of a unit under this TMPDIR does not fit.
# Needs no root. Put it before nonroot.sh: the directory has mode 1777,
# and nonroot.sh keeps TMPDIR.
set -eu
[ "${1:-}" = "--" ] && shift
len=${LAB_TMP_LEN:-100}
base=${LAB_TMP_BASE:-/tmp/lab-long-tmp-}
pad=$((len - ${#base}))
[ "$pad" -ge 0 ] || { echo "lab long-tmp: $base has more than $len bytes" >&2; exit 64; }
dir=$base$(printf "%${pad}s" "" | tr ' ' t)
mkdir -p "$dir"
chmod 1777 "$dir"
# The check: the path has len bytes, and it is a directory that all can write.
got=$(printf '%s' "$dir" | wc -c | tr -d ' ')
[ "$got" -eq "$len" ] || { echo "lab long-tmp: $dir has $got bytes, not $len" >&2; exit 70; }
[ "$(stat -c %a "$dir")" = 1777 ] || { echo "lab long-tmp: $dir does not have mode 1777" >&2; exit 70; }
echo "lab long-tmp: TMPDIR=$dir ($got bytes)" >&2
TMPDIR=$dir exec "$@"
