#!/bin/bash
# Q4: RLIMIT_FSIZE x .NET W^X. Does `Runner.Listener --version` survive the file-size limit, native (control, no podvm) and
# inside podvm (userland rung, --unsafe-host-paths, the only way the runner gets past CoreCLR init here, see 03)?
# Also: does podvm have its own limit flags (--ulimit, -m, --podvm-mem)? And the host default from /proc/self/limits.
# Needs 03 to have been run once ($WORK/runner, $WORK/drootfs). Exit 0 = ran; 1 = W^X=0 did not make the runner survive the
# limit (the expectation: it survives); 2 = no runner payload.
. "$(dirname "$0")/lib.sh"; need
[ -x "$WORK/runner/bin/Runner.Listener" ] || { echo "run 03 first"; exit 2; }
echo "date=$(date -u +%FT%TZ) kernel=$(uname -r) cpus=$(nproc) virt=$(systemd-detect-virt)"
echo "## host default from /proc/self/limits"; grep -i "file size" /proc/self/limits
echo "## podvm's own limit flags (parity rows)"
R="$PODVM run --rm -q"
run_case ulimit-flag $R --ulimit fsize=1048576 alpine true
run_case memory-flag $R -m 100m alpine true
run_case podvm-mem $R --podvm-mem 1G alpine true
grep -c . /dev/null >/dev/null
echo "## control: prlimit reaches an alpine payload through podvm (dd 3 MiB under --fsize=1 MiB)"
run_case alpine-dd-1MiB prlimit --fsize=1048576 $R alpine sh -c 'dd if=/dev/zero of=/tmp/x bs=1M count=3 2>&1; echo dd-rc=$?'
run_case alpine-dd-1MiB-rep prlimit --fsize=1048576 $R alpine sh -c 'dd if=/dev/zero of=/tmp/x bs=1M count=3 2>&1; echo dd-rc=$?'
export DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1
echo "## native control, no podvm (host default limit = hard limit, cannot be raised here)"
NATIVE_FAIL=0
for lim in default 1073741824 67108864 1048576; do for w in 1 0; do for rep in 1 2; do
  if [ $lim = default ]; then pre=""; else pre="prlimit --fsize=$lim --"; fi
  ( cd "$WORK/runner" && DOTNET_EnableWriteXorExecute=$w $pre ./bin/Runner.Listener --version >"$WORK/o.txt" 2>"$WORK/e.txt" ); rc=$?
  echo "native fsize=$lim W^X=$w rep=$rep exit=$rc out=$(head -c 30 "$WORK/o.txt")"
  [ $w = 0 ] && [ $rc != 0 ] && NATIVE_FAIL=1
done; done; done
echo "## mechanism (native, strace): what does W^X do that FSIZE stops?"
if command -v strace >/dev/null; then
  ( cd "$WORK/runner" && strace -f -e trace=ftruncate,memfd_create -o "$WORK/st.txt" ./bin/Runner.Listener --version >/dev/null 2>&1 ); echo "strace run exit=$?"
  grep -E "memfd_create|ftruncate|SIGXFSZ" "$WORK/st.txt" | head -4
else echo "no strace: mechanism not re-observed"; fi
echo "## can the limit be raised above the host hard limit? (sudo prlimit)"
sudo -n prlimit --fsize=unlimited:unlimited -- true; echo "sudo prlimit unlimited exit=$?"
echo "## inside podvm"
FAIL=$NATIVE_FAIL
sudo -n true 2>/dev/null && sudo -n mkdir /opt/runner 2>/dev/null || { echo "cannot create host /opt/runner: inside-podvm cases skipped"; exit 2; }
sudo -n cp -a "$WORK/runner/." /opt/runner/ && sudo -n chown -R "$(id -u)" /opt/runner
D=$WORK/drootfs
sudo -n mkdir -p "$D/opt/runner" && sudo -n cp -a "$WORK/runner/." "$D/opt/runner/"
hack_on "$D" "$WORK/runner"
sudo -n tar -cf "$WORK/img.tar" -C "$D" . && sudo -n chown "$(id -u)" "$WORK/img.tar"
"$PODVM" rmi podrunner-hack:t >/dev/null 2>&1; "$PODVM" import "$WORK/img.tar" podrunner-hack:t 2>&1 | tail -1; rm -f "$WORK/img.tar"
hack_off "$D"; sudo -n rm -rf "$D/opt/runner"
U="$R --unsafe-host-paths"
V=/opt/runner/bin/Runner.Listener
for rep in 1 2; do
  run_case "podvm host-default W^X=1 rep$rep" $U podrunner-hack:t $V --version
  run_case "podvm host-default W^X=0 rep$rep" $U -e DOTNET_EnableWriteXorExecute=0 podrunner-hack:t $V --version; [ $LAST_RC = 0 ] || FAIL=1
  run_case "podvm prlimit-1GiB W^X=1 rep$rep" prlimit --fsize=1073741824 $U podrunner-hack:t $V --version
  run_case "podvm prlimit-1GiB W^X=0 rep$rep" prlimit --fsize=1073741824 $U -e DOTNET_EnableWriteXorExecute=0 podrunner-hack:t $V --version; [ $LAST_RC = 0 ] || FAIL=1
  run_case "podvm prlimit-1MiB W^X=0 rep$rep" prlimit --fsize=1048576 $U -e DOTNET_EnableWriteXorExecute=0 podrunner-hack:t $V --version
done
echo "## same cases on the default (anchored) rung: blocked before the limit matters"
run_case "podvm anchored W^X=0" prlimit --fsize=1073741824 $R -e DOTNET_EnableWriteXorExecute=0 podrunner-hack:t $V --version
"$PODVM" rmi podrunner-hack:t >/dev/null 2>&1; sudo -n rm -rf /opt/runner
exit $FAIL
