#!/bin/bash
# Minimal container hook for the official actions/runner. Logs the stdin JSON line to $HOOK_LOG, writes a minimal response.
# Contract (runner source, Runner.Worker/Container/ContainerHooks): stdin = ONE line of JSON {command,responseFile,args,state};
# exit 0 = success; the runner has already created an EMPTY file at responseFile, the hook APPENDS the response JSON to it.
set -eu
IFS= read -r line
printf '%s\n' "$line" >> "${HOOK_LOG:-/dev/null}"
cmd=$(printf '%s' "$line" | grep -o '"command":"[a-z_]*"' | head -1 | cut -d'"' -f4)
resp=$(printf '%s' "$line" | grep -o '"responseFile":"[^"]*"' | head -1 | cut -d'"' -f4)
case "$cmd" in
  prepare_job)
    # isAlpine is required when args.container is present; context.services is a LIST in the order of args.services.
    printf '%s\n' '{"state":{"example":"1"},"context":{"container":{"id":"c1","network":"n1"},"services":[]},"isAlpine":false}' >> "$resp" ;;
  run_script_step|run_container_step|cleanup_job)
    : ;;   # no response needed; an empty response file is accepted
  *) echo "unknown command: $cmd" >&2; exit 1 ;;
esac
exit 0
