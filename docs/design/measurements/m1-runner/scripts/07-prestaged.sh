#!/bin/bash
# Q7: can the runner start from PRE-STAGED credential files, with no --jitconfig and no ACTIONS_RUNNER_INPUT_JITCONFIG?
# The synthetic blob is decoded into .runner/.credentials/.credentials_rsaparams in a fresh copy of the runner root, then
# `./run.sh` (or Runner.Listener run) is started with nothing else. Checks: same requests as the blob run, files rewritten or
# deleted, modes after, exit code, and whether argv/environ of any process in the tree hold the blob, key or sentinels.
# Exit 1 if a staged run does not reach POST /session or if argv/environ hold the blob/sentinels.
. "$(dirname "$0")/lib.sh"
start_log 07-prestaged
conditions; need_runner
fail=0
for round in 1 2; do
  for form in staged-runsh staged-listener staged400-runsh; do
    run_scn "07-$form-$round" "$form" ok 7 || { echo "could not run"; exit 2; }
    summarize_reqlog "$SCN_OUT/req.log" | grep -q '^POST   /session' || { echo "EXPECTATION FAILED: no POST /session ($form $round)"; fail=1; }
  done
done
pristine_intact || fail=1
exit $fail
