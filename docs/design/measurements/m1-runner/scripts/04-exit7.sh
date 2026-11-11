#!/bin/bash
# Q4: exit code 7 (RunnerVersionDeprecated). The fake server answers GET /message with the broker error body the runner maps to
# AccessDeniedException{ErrorCode=1} (BrokerHttpClient.cs:120-124: source "actions-broker-listener", errorKind "RunnerVersionTooOld").
# Matrix: listener direct vs run.sh, ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE in {unset, true, 1}.
# Also: POST /session answering 403 (the place R4 named): does it yield 7?
# Exit 1 if the listener direct run with =true does not return 7.
. "$(dirname "$0")/lib.sh"
start_log 04-exit7
conditions; need_runner
V=ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE
declare -A rc
for round in 1 2; do
  run_scn "04-l-unset-$round" arg-listener tooold 4;                      rc[l-unset-$round]=$SCN_RC
  EXTRA_ENV="$V=true" run_scn "04-l-true-$round" arg-listener tooold 4;   rc[l-true-$round]=$SCN_RC
  EXTRA_ENV="$V=1" run_scn "04-l-1-$round" arg-listener tooold 4;         rc[l-1-$round]=$SCN_RC
  run_scn "04-r-unset-$round" arg-runsh tooold 4;                         rc[r-unset-$round]=$SCN_RC
  EXTRA_ENV="$V=true" run_scn "04-r-true-$round" arg-runsh tooold 4;      rc[r-true-$round]=$SCN_RC
  EXTRA_ENV="$V=1" run_scn "04-r-1-$round" arg-runsh tooold 4;            rc[r-1-$round]=$SCN_RC
done
EXTRA_ENV="$V=true" run_scn 04-session403 arg-listener session403 14;    rc[session403]=$SCN_RC
echo; echo "=== SUMMARY exit codes (listener direct = l, run.sh = r)"
for k in "${!rc[@]}"; do echo "$k ${rc[$k]}"; done | sort
[ "${rc[l-true-1]}" = 7 ] && [ "${rc[l-true-2]}" = 7 ] || { echo "EXPECTATION FAILED: listener with $V=true did not return 7"; exit 1; }
pristine_intact || exit 1
exit 0
