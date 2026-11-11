#!/bin/bash
# Run every numbered script in order. Each script writes its own logs/NN-name.log (with the exit=N line); this prints the codes.
# 03, 04 and 07 start the runner against a local fake server and take about 2 minutes each. Needs: curl, python3, openssl, strace,
# prlimit, readelf, objdump, unshare (user namespaces). Everything but 00 is offline.
HERE=$(cd "$(dirname "$0")" && pwd)
for s in "$HERE"/[0-9][0-9]-*.sh; do
  n=$(basename "$s" .sh)
  "$s" >/dev/null 2>&1; echo "$n exit=$?"
done
