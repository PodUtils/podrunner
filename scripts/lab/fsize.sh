#!/bin/sh
# SPDX-License-Identifier: 0BSD
# The class fsize (T-638): the soft and the hard limit of file size of the
# command are LAB_FSIZE bytes (default 1 GiB, 1073741824), as under
# `ulimit -f 1048576` in bash. The hard limit stays: a user with no root
# cannot raise it. prlimit counts bytes; the `ulimit -f` of sh counts blocks
# of 512 bytes, the one of bash blocks of 1024. Needs no root; put it
# after nonroot.sh: sudo (pam_limits) resets the limit of its command.
set -eu
[ "${1:-}" = "--" ] && shift
size=${LAB_FSIZE:-1073741824}
command -v prlimit >/dev/null 2>&1 || { echo "lab fsize: no prlimit (util-linux)" >&2; exit 69; }
# The check: a command under prlimit has the soft and the hard limit.
got=$(prlimit --fsize="$size:$size" -- prlimit --fsize --noheadings --output SOFT,HARD | tr -s ' ' ' ')
[ "$got" = "$size $size" ] || { echo "lab fsize: the limit is '$got', not '$size $size'" >&2; exit 70; }
echo "lab fsize: the soft and the hard limit of file size are $size bytes" >&2
exec prlimit --fsize="$size:$size" -- "$@"
