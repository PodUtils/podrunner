#!/bin/bash
# Q3: what the official runner does with a SYNTHETIC jitconfig blob (argv form and env form), offline against a local fake server.
# What this establishes: which files it writes (mode, owner, persistence), whether argv/environ of the listener and its parents
# hold the blob, which HTTP requests it makes, the exit code. It does NOT run a job (no Worker), see README.
# Exit 1 if the runner never reached POST /session in a scenario that should.
. "$(dirname "$0")/lib.sh"
start_log 03-jitconfig
conditions; need_runner
fail=0
for round in 1 2; do
  for form in arg-runsh env-runsh arg-listener env-listener; do
    run_scn "03-$form-$round" "$form" ok 7 || { echo "scenario could not run"; exit 2; }
    grep -q 'POST /session\|^POST  */session' <(summarize_reqlog "$SCN_OUT/req.log") || { echo "EXPECTATION FAILED: no POST /session in $form round $round"; fail=1; }
  done
done
echo; echo "################ controls"
run_scn 03-ctl-garbage garbage-listener ok 3
run_scn 03-ctl-bare bare-runsh ok 4
pristine_intact || fail=1
exit $fail
