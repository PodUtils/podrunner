#!/bin/sh
# The container hook relay of podrunner (GitHub #35). The official runner
# runs this script for each container operation, with one JSON object on
# stdin. The script posts it to the control socket of the unit
# ($PODRUNNER_CTL) and checks nothing: the host checks each call. The answer
# of prepare_job goes to the response file; the output of a step goes to
# stdout while the step runs (T-694). The exit code is the exit code that
# the host sends in the trailer of the answer.
set -eu
dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
cat > "$dir/in"
# The runner writes "command" and "responseFile" as plain strings.
cmd=$(sed -n 's/^{"command":"\([a-z_]*\)".*/\1/p' "$dir/in")
out=$(sed -n 's/.*"responseFile":"\([^"]*\)".*/\1/p' "$dir/in")
[ -n "$cmd" ] || { echo "podrunner-hook: no command in the input" >&2; exit 2; }
# The live route: the body comes in chunks as the step writes it, and the
# exit code or the refusal comes in a trailer, which curl writes to the
# head file. An HTTP error (a request that the host cannot read) gives no
# body to the step: curl names the status on stderr and exits 22.
post() {
  curl -fsS -N --unix-socket "$PODRUNNER_CTL" -D "$dir/head" \
    -H 'Content-Type: application/json' --data-binary "@$dir/in" "http://unit/v2/hook/$cmd"
}
rc=0
if [ "$cmd" = prepare_job ] && [ -n "$out" ]; then
  post >> "$out" || rc=$?
else
  post || rc=$?
fi
if [ "$rc" = 22 ]; then
  echo "podrunner-hook: $cmd refused" >&2
  exit 4
fi
[ "$rc" = 0 ] || exit 3
head=$(tr -d '\r' < "$dir/head")
status=$(printf '%s\n' "$head" | sed -n '1s/^HTTP\/[0-9.]* \([0-9]*\).*/\1/p')
refusal=$(printf '%s\n' "$head" | sed -n 's/^[Xx]-[Pp]odrunner-[Rr]efusal: *//p')
if [ "$status" != 200 ] || [ -n "$refusal" ]; then
  echo "podrunner-hook: $cmd refused: ${refusal:-HTTP $status}" >&2
  exit 4
fi
code=$(printf '%s\n' "$head" | sed -n 's/^[Xx]-[Pp]odrunner-[Ee]xit: *\([0-9-]*\)$/\1/p')
exit "${code:-5}"
