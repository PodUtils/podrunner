#!/bin/sh
# The container hook relay of podrunner (GitHub #35). The official runner
# runs this script for each container operation, with one JSON object on
# stdin. The script posts it to the control socket of the unit
# ($PODRUNNER_CTL) and checks nothing: the host checks each call. The answer
# of prepare_job goes to the response file; the output of a step goes to
# stdout. The exit code is the exit code that the host sends.
set -eu
dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
cat > "$dir/in"
# The runner writes "command" and "responseFile" as plain strings.
cmd=$(sed -n 's/^{"command":"\([a-z_]*\)".*/\1/p' "$dir/in")
out=$(sed -n 's/.*"responseFile":"\([^"]*\)".*/\1/p' "$dir/in")
[ -n "$cmd" ] || { echo "podrunner-hook: no command in the input" >&2; exit 2; }
status=$(curl -sS --unix-socket "$PODRUNNER_CTL" -o "$dir/body" -D "$dir/head" -w '%{http_code}' \
  -H 'Content-Type: application/json' --data-binary "@$dir/in" "http://unit/v1/hook/$cmd") || exit 3
if [ "$status" != 200 ]; then
  echo "podrunner-hook: $cmd refused: $(cat "$dir/body")" >&2
  exit 4
fi
if [ "$cmd" = prepare_job ] && [ -n "$out" ]; then
  cat "$dir/body" >> "$out"
else
  cat "$dir/body"
fi
code=$(tr -d '\r' < "$dir/head" | sed -n 's/^[Xx]-[Pp]odrunner-[Ee]xit: *\([0-9-]*\)$/\1/p')
exit "${code:-5}"
