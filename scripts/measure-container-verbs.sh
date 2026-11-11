#!/bin/sh
# Measure U19 of the hook path (T-034) on the podvm rung of this host: the
# podvm verbs of the four hook commands, with the mount tables of the
# runner. A job container of an image other than the runner image runs
# with the seven job-container mounts (`run -d -v`), each step is an
# `exec -e -w` (timed over 20 steps), a container action runs with the
# five action mounts, and `rm -f` ends both. Run it as root and as a uid
# with no user namespace.
#
#   sh scripts/measure-container-verbs.sh PODVM WORKDIR
#
# It prints one line for each call: "CALL ok" or "CALL no: why".
set -u
podvm=$1
work=$2
here=$(cd "$(dirname "$0")" && pwd)
export PODVM_STORE="$work/store"
mkdir -p "$work/runner/_work/_temp/_github_home" "$work/runner/_work/_temp/_github_workflow" \
  "$work/runner/_work/_temp/_runner_file_commands" "$work/runner/_work/_actions" "$work/runner/_work/_tool" \
  "$work/runner/_work/app/app" "$work/runner/externals"
"$podvm" probe 2>&1 | sed -n '2s/^/rung: /p'
python3 -I "$here/stand-in-image.py" "$work/image.tar" > /dev/null
"$podvm" load -i "$work/image.tar" > /dev/null 2>&1 || { echo "load no"; exit 1; }
image="glibc-mini:1"
r="$work/runner"

say() {
  if [ "$2" -eq 0 ]; then echo "$1 ok"; else echo "$1 no: $(tail -1 "$work/out" | cut -c1-200)"; fi
}

# prepare_job: the job container, with the job-container table.
"$podvm" rm -f job > /dev/null 2>&1
"$podvm" run -d --name job \
  -v "$r/_work:/__w" -v "$r/_work/_temp:/__w/_temp" -v "$r/_work/_actions:/__w/_actions" \
  -v "$r/_work/_tool:/__w/_tool" -v "$r/externals:/__e:ro" \
  -v "$r/_work/_temp/_github_home:/github/home" -v "$r/_work/_temp/_github_workflow:/github/workflow" \
  "$image" /bin/sleep 600 > "$work/out" 2>&1
say prepare-job-run-d $?

# run_script_step: exec with an environment and a working directory.
"$podvm" exec -e STEP=one -w /__w/app/app job /bin/sh -c 'test "$STEP" = one && test "$(pwd)" = /__w/app/app && echo seen > /__w/app/app/step.txt' > "$work/out" 2>&1
rc=$?
[ "$rc" -eq 0 ] && [ -f "$r/_work/app/app/step.txt" ] || rc=1
say exec-env-workdir-and-mount $rc

start=$(date +%s%N)
n=0
while [ "$n" -lt 20 ]; do
  "$podvm" exec job /bin/echo > /dev/null 2>&1 || break
  n=$((n + 1))
done
end=$(date +%s%N)
echo "exec-20-steps $n done, $(( (end - start) / 20000000 )) ms each"

# /__e is read-only in the job container.
"$podvm" exec job /bin/sh -c 'echo x > /__e/write.txt' > "$work/out" 2>&1
rc=$?; if [ "$rc" -ne 0 ] && [ ! -f "$r/externals/write.txt" ]; then echo "externals-read-only ok"; else echo "externals-read-only no: exit $rc, $(ls "$r/externals")"; fi

# run_container_step: an action with the five action mounts and its logs.
"$podvm" rm -f action > /dev/null 2>&1
"$podvm" run --name action \
  -v "$r/_work/_temp:/github/runner_temp" -v "$r/_work/_temp/_github_home:/github/home" \
  -v "$r/_work/_temp/_github_workflow:/github/workflow" \
  -v "$r/_work/_temp/_runner_file_commands:/github/file_commands" -v "$r/_work/app/app:/github/workspace" \
  -w /github/workspace "$image" /bin/sh -c 'cat step.txt; exit 3' > "$work/out" 2>&1
rc=$?
grep -q seen "$work/out" && [ "$rc" -eq 3 ] && say run-container-step-exit-and-output 0 || say run-container-step-exit-and-output 1
"$podvm" logs action > "$work/out" 2>&1
grep -q seen "$work/out" && say logs-of-the-action 0 || say logs-of-the-action 1

# cleanup_job.
"$podvm" rm -f action job > "$work/out" 2>&1
say cleanup-rm-f $?
