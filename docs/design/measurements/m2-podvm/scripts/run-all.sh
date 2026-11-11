#!/bin/bash
# Run every numbered script, write logs/NN-name.log with the exit code line read from the process.
HERE=$(cd "$(dirname "$0")" && pwd)
for s in "$HERE"/[0-9][0-9]-*.sh; do
  n=$(basename "$s" .sh)
  [ -n "$ONLY" ] && [ "$ONLY" != "${n%%-*}" ] && continue
  "$s" >"$HERE/../logs/$n.log" 2>&1
  rc=$?
  sed -i -e "s#${WORK:-/nonexistent-work}#\$WORK#g" -e "s#$HERE/../../../repos#\$REPOS#g" "$HERE/../logs/$n.log"
  sed -i -e "s#$HERE#scripts#g" "$HERE/../logs/$n.log"
  echo "exit=$rc" >>"$HERE/../logs/$n.log"
  echo "$n exit=$rc"
done
