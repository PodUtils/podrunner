#!/bin/sh
# SPDX-License-Identifier: 0BSD
# The free space and the time at each step of hosted-measure.yml (T-557).
#
#   sh scripts/hosted/space.sh mark DIR STEP RC
#   sh scripts/hosted/space.sh report DIR
#
# mark adds one row to DIR/space.tsv: the step, its exit code, the seconds
# since the first mark and since the mark before, and the size, the use and
# the free bytes (df -B1) of /, /mnt and the file system of incusd (POOL,
# default /var/lib/incus; "-" while the path is absent).
# It prints the row too: the log keeps it when the disk is full. It adds
# the view of incusd (incus storage info) to DIR/pool.txt.
# report writes the table in Markdown, in GiB.
set -eu
cmd=${1:-}
dir=${2:-}
[ -n "$dir" ] || { echo "usage: space.sh mark DIR STEP RC | report DIR" >&2; exit 64; }

# The size, the use and the free bytes of the file system of $1, or "- - -".
df3() {
    out=$(df -B1 --output=size,used,avail "$1" 2> /dev/null || sudo -n df -B1 --output=size,used,avail "$1" 2> /dev/null) || out=""
    row=$(printf '%s\n' "$out" | sed -n 2p)
    if [ -n "$row" ]; then
        # shellcheck disable=SC2086 # split the three numbers into fields
        printf '%s\t%s\t%s' $row
    else
        printf -- '-\t-\t-'
    fi
}

case $cmd in
mark)
    step=${3:?the step}
    rc=${4:?the exit code}
    mkdir -p "$dir"
    now=$(date +%s)
    [ -s "$dir/start" ] || echo "$now" > "$dir/start"
    [ -s "$dir/last" ] || echo "$now" > "$dir/last"
    total=$((now - $(cat "$dir/start")))
    delta=$((now - $(cat "$dir/last")))
    echo "$now" > "$dir/last"
    pool=${POOL:-/var/lib/incus}
    if [ -e "$pool" ]; then p=$(df3 "$pool"); else p=$(printf -- '-\t-\t-'); fi
    t=$(printf '\t')
    [ -s "$dir/space.tsv" ] || printf 'step\trc\tseconds\tstep_seconds\troot_size\troot_used\troot_free\tmnt_size\tmnt_used\tmnt_free\tincusd_size\tincusd_used\tincusd_free\n' > "$dir/space.tsv"
    line="$step$t$rc$t$total$t$delta$t$(df3 /)$t$(df3 /mnt)$t$p"
    echo "space: $line"
    printf '%s\n' "$line" >> "$dir/space.tsv"
    if command -v incus > /dev/null 2>&1; then
        { echo "--- $step"; sudo -n incus storage info default --bytes 2>&1 | head -n 20; } >> "$dir/pool.txt" || true
    fi
    ;;
report)
    [ -s "$dir/space.tsv" ] || { echo "No row of space: no step ran."; exit 0; }
    awk -F'\t' '
        function g(b) { return b == "-" ? "-" : sprintf("%.1f", b / 1073741824) }
        NR == 1 { next }
        NR == 2 { print "| Step | rc | s | step s | / used | / free | /mnt used | /mnt free | incusd used | incusd free |"
                  print "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |" }
        { printf "| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |\n", $1, $2, $3, $4, g($6), g($7), g($9), g($10), g($12), g($13)
          r = g($5); m = g($8); i = g($11) }
        END { printf "\nThe sizes in GiB at the last step: / %s, /mnt %s, the file system of incusd %s.\n", r, m, i }
    ' "$dir/space.tsv"
    ;;
*)
    echo "usage: space.sh mark DIR STEP RC | report DIR" >&2
    exit 64
    ;;
esac
