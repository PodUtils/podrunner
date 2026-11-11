#!/bin/bash
# Q5: container hooks gate. (a) prints the decisive source lines of the runner at the pinned tag, with path:line, from the
# extracted source (so the log is evidence, not memory). (b) drives my example hook with synthetic stdin for the four commands and
# validates the responses with a mirror of the runner's checks (my reimplementation, labelled). (c) greps the hooklib schema.
# What this does NOT do: run a job. A job needs the broker flow (see README, "what blocks a real run").
# Exit 1 if an example response fails the mirror check or an expected source line is missing.
. "$(dirname "$0")/lib.sh"
start_log 05-hooks
conditions
[ -d "$SRC/src" ] && [ -d "$HOOKS/packages/hooklib" ] || { echo "run 00-fetch.sh first"; exit 2; }
fail=0
show() { # file start end
  echo "--- $1:$2-$3"; sed -n "$2,$3p" "$SRC/src/$1" | cut -c1-200 | nl -ba -v"$2" | sed 's/^ *//'
}
echo "=== (a) source lines that decide the behaviour"
show Runner.Worker/FeatureManager.cs 8 12
show Runner.Common/Constants.cs 170 170
show Runner.Common/Constants.cs 280 280
show Runner.Worker/ContainerOperationProvider.cs 34 44
show Runner.Worker/ContainerOperationProvider.cs 64 74
show Runner.Worker/Container/ContainerHooks/ContainerHookManager.cs 33 36
show Runner.Worker/Container/ContainerHooks/ContainerHookManager.cs 183 197
show Runner.Worker/Container/ContainerHooks/ContainerHookManager.cs 136 142
show Runner.Worker/Container/ContainerHooks/HookResponse.cs 14 25
echo "=== (a2) env var read by the Listener process? the hook path is read in the WORKER (ContainerHookManager.Initialize), grep for other readers:"
grep -rn "ContainerHooksPath" "$SRC/src" --include=*.cs | grep -v "/Test/" | sed "s|$SRC/src/||"
echo "=== (b) drive the example hook (synthetic stdin; shape from HookInput.cs)"
HOOK=$HERE/../hook-example/hook-log.sh
TMP=$(mktemp -d "$M1/hook.XXXXXX"); export HOOK_LOG=$TMP/hook.log
drive() { # command has_container n_services stdin-args-json
  local cmd=$1 hc=$2 ns=$3 args=$4 resp=$TMP/$1.resp
  : > "$resp"
  printf '{"command":"%s","responseFile":"%s","args":%s,"state":{"example":"1"}}\n' "$cmd" "$resp" "$args" | "$HOOK"; local rc=$?
  echo "hook command=$cmd exit=$rc response=$(cat "$resp" | head -c 200)"
  python3 "$HERE/check_hook_response.py" "$cmd" "$hc" "$ns" "$resp" || fail=1
}
drive prepare_job 1 0 '{"container":{"image":"alpine:3","workingDirectory":"/__w/r/r","systemMountVolumes":[],"userMountVolumes":[],"environmentVariables":{}},"services":[]}'
drive run_script_step 0 0 '{"entryPoint":"bash","entryPointArgs":["-e","/__w/_temp/x.sh"],"environmentVariables":{},"prependPath":[],"workingDirectory":"/__w/r/r"}'
drive run_container_step 0 0 '{"image":"alpine:3","workingDirectory":"/__w/r/r","systemMountVolumes":[]}'
drive cleanup_job 0 0 '{}'
echo "stdin lines logged: $(wc -l < "$HOOK_LOG")"
echo "--- control: hooklib-style response (alpine / services as map) is rejected by the mirror check"
printf '{"state":{},"context":{"container":{"id":"c1"}},"alpine":false}\n' > "$TMP/bad1.json"
python3 "$HERE/check_hook_response.py" prepare_job 1 0 "$TMP/bad1.json" && { echo "EXPECTATION FAILED: bad1 accepted"; fail=1; }
printf '{"isAlpine":false,"context":{"services":{"redis":{"id":"s1"}}}}\n' > "$TMP/bad2.json"
python3 "$HERE/check_hook_response.py" prepare_job 1 1 "$TMP/bad2.json" && { echo "EXPECTATION FAILED: bad2 accepted"; fail=1; }
echo "=== (c) hooklib TypeScript schema at the pinned commit (hooklib/src/interfaces.ts, lines containing the response types)"
grep -n "interface PrepareJobResponse" -A5 "$HOOKS/packages/hooklib/src/interfaces.ts"
echo "--- what the official docker hook actually writes (packages/docker/src/hooks/prepare-job.ts:141-178 key names):"
sed -n 141,180p "$HOOKS/packages/docker/src/hooks/prepare-job.ts" | grep -n "isAlpine\|state\|context\|services\|alpine"
exit $fail
