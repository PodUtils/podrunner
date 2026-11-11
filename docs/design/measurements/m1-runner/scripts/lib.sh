# Shared helpers, sourced by NN-*.sh. Not a script of its own.
# Exit contract for every script: 0 = ran, 1 = ran and an expectation failed, 2 = could not run.
HERE=$(cd "$(dirname "${BASH_SOURCE[1]}")" && pwd)
OUT=$(cd "$HERE/.." && pwd)
WORK=${WORK:-${TMPDIR:-/tmp}/podrunner-m}
M1=$WORK/m1-runner
TAG=v2.338.2
TARBALL_URL=https://github.com/actions/runner/releases/download/$TAG/actions-runner-linux-x64-2.338.2.tar.gz
TARBALL_SHA256=baebf204307bac6193480852732f0aba2ae97b6c5597c244f4d5558390147c40
TARBALL_FALLBACK_URL=https://fetch.web.pkgforge.dev/$TARBALL_URL
SRC_URL=https://github.com/actions/runner/archive/refs/tags/$TAG.tar.gz
HOOKS_URL=https://github.com/actions/runner-container-hooks/archive/d4731a43a2fe.tar.gz
HOOKS_SHA256=fc026e8c1fb41b9f0aad53c5adccfba26732b1a35387e90ca60a5338550a734b   # archive of commit d4731a43a2fe (R4 pin), observed 2026-11-10
HOOKS=$M1/hooks
RUNNER=$M1/r        # pristine extracted runner, never run in place by scripts that write
SRC=$M1/src         # runner source at the pinned tag
mkdir -p "$OUT/logs" "$M1"

# start_log NAME: send all output to logs/NAME.log and append "exit=N" when the script ends.
start_log() {
  LOG=$OUT/logs/$1.log
  exec >"$LOG" 2>&1
  trap 'rc=$?; echo "exit=$rc"' EXIT
}

conditions() {
  echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "kernel: $(uname -r)  cpus: $(nproc)  virt: $(systemd-detect-virt 2>&1)  uid: $(id -u)"
  echo "os: $(. /etc/os-release; echo "$PRETTY_NAME")  glibc: $(ldd --version | head -1)"
  echo "fsize limit (soft/hard, bytes): $(prlimit --fsize --noheadings --raw | awk '{print $(NF-2)"/"$(NF-1)}')"
}

need_runner() {
  [ -x "$RUNNER/bin/Runner.Listener" ] || { echo "runner not extracted in $RUNNER; run 00-fetch.sh first"; exit 2; }
}

# Run the listener with .NET W^X off (the only mode that starts under this host's 20 GiB fsize hard limit).
export_wx_off() { export DOTNET_EnableWriteXorExecute=0; }

# fresh_copy NAME: a private, writable copy of the pristine runner at $M1/run/NAME (cp -al: directories are real, files are hard links to the pristine tree; the runner only creates new files, and pristine_intact checks that). Prints its path.
# Scripts that start the runner must use a copy: the runner writes .env, .path, _diag, run-helper.sh and the JIT files into its root.
fresh_copy() {
  local d=$M1/run/$1
  rm -rf "$d"; mkdir -p "$M1/run"
  cp -al "$RUNNER" "$d" || return 1
  echo "$d"
}

# ---- fake server and scenario driver (used by 03, 04, 07) ----
# start_server MODE -> sets SRV_PID PORT REQLOG; stop_server kills it.
start_server() {
  local mode=$1
  SRVDIR=$(mktemp -d "$M1/srv.XXXXXX"); REQLOG=$SRVDIR/req.log; : > "$REQLOG"
  python3 "$HERE/fakeserver.py" "$SRVDIR/port" "$REQLOG" "$mode" >"$SRVDIR/server.out" 2>&1 & SRV_PID=$!
  for _ in $(seq 50); do [ -s "$SRVDIR/port" ] && break; sleep 0.1; done
  PORT=$(cat "$SRVDIR/port") || return 2
}
stop_server() { kill "$SRV_PID" 2>/dev/null; wait "$SRV_PID" 2>/dev/null; }
# summarize_reqlog FILE: method, path, query keys, status, which secret-bearing headers were present (values never shown)
summarize_reqlog() {
  python3 - "$1" <<'PY'
import json, sys
for l in open(sys.argv[1]):
    r = json.loads(l)
    h = r["headers"]
    auth = h.get("Authorization", "none")
    print("%-6s %-22s q=%s -> %s | auth=%s | body=%s" % (r["method"], r["path"], ",".join(sorted(r["query"])) or "-", r["status"], auth, r["body"][:110].replace("\n", " ")))
PY
}
# file_state DIR: mode owner size of the runner config files
file_state() {
  for f in .runner .credentials .credentials_rsaparams .credentials_migrated .runner_migrated .env .path; do
    [ -e "$1/$f" ] && stat -c "%n mode=%a owner=%U:%G size=%s" -- "$1/$f" | sed "s|^$1/||"
  done
}

# decode_blob BLOBFILE DIR [MODE]: write the three files of a jitconfig blob into DIR (what Runner.cs:234-256 does), chmod MODE if given.
decode_blob() {
  python3 - "$1" "$2" "${3:-}" <<'PY'
import base64, json, os, sys
blob, d, mode = sys.argv[1], sys.argv[2], sys.argv[3]
files = json.loads(base64.b64decode(open(blob).read()))
for k, v in files.items():
    p = os.path.join(d, k)
    open(p, "wb").write(base64.b64decode(v))
    if mode:
        os.chmod(p, int(mode, 8))
PY
}

# run_scn NAME FORM SERVERMODE [SECONDS]
# FORM: arg-runsh | env-runsh | arg-listener | env-listener | staged-runsh | staged-listener | staged400-runsh | bare-runsh
# Starts the runner in its own session against the fake server, samples /proc, SIGTERMs the session, records exit code.
# Sets SCN_RC (exit code of the started command) and SCN_DIR. Output goes to the log.
run_scn() {
  local name=$1 form=$2 smode=$3 secs=${4:-7} pid d BLOB
  echo; echo "################ scenario $name: form=$form server=$smode"
  d=$(fresh_copy "$name") || return 2; SCN_DIR=$d
  start_server "$smode" || return 2
  python3 "$HERE/mkjit.py" "$PORT" "$SRVDIR" > /dev/null || return 2
  BLOB=$(cat "$SRVDIR/jitconfig.b64")
  export_wx_off; unset ACTIONS_RUNNER_INPUT_JITCONFIG
  [ -n "${EXTRA_ENV:-}" ] && export $EXTRA_ENV
  cd "$d" || return 2
  case $form in
    staged-*)  decode_blob "$SRVDIR/jitconfig.b64" "$d" 600 ;;
    staged400-*) decode_blob "$SRVDIR/jitconfig.b64" "$d" 400 ;;
  esac
  case $form in
    staged-*|staged400-*|bare-*) sha256sum .runner .credentials .credentials_rsaparams 2>/dev/null | sed 's/^\(.\{12\}\).*  /\1...  /' > "$SRVDIR/sha.before" ;;
  esac
  echo "files before start:"; file_state "$d"
  case $form in
    arg-runsh)        setsid ./run.sh --jitconfig "$BLOB" > "$SRVDIR/out.txt" 2>&1 & ;;
    env-runsh)        ACTIONS_RUNNER_INPUT_JITCONFIG=$BLOB setsid ./run.sh > "$SRVDIR/out.txt" 2>&1 & ;;
    arg-listener)     setsid ./bin/Runner.Listener run --jitconfig "$BLOB" > "$SRVDIR/out.txt" 2>&1 & ;;
    env-listener)     ACTIONS_RUNNER_INPUT_JITCONFIG=$BLOB setsid ./bin/Runner.Listener run > "$SRVDIR/out.txt" 2>&1 & ;;
    garbage-listener) setsid ./bin/Runner.Listener run --jitconfig "not-valid-base64!!" > "$SRVDIR/out.txt" 2>&1 & ;;
    staged-runsh|staged400-runsh|bare-runsh) setsid ./run.sh > "$SRVDIR/out.txt" 2>&1 & ;;
    staged-listener)  setsid ./bin/Runner.Listener run > "$SRVDIR/out.txt" 2>&1 & ;;
    *) echo "unknown form $form"; return 2 ;;
  esac
  pid=$!
  ( sleep $((secs + 25)); kill -KILL -- -"$pid" 2>/dev/null ) & local wd=$!
  python3 "$HERE/sampler.py" "$pid" "$secs" "$SRVDIR/jitconfig.b64" "$SRVDIR/rsa.d.b64"
  echo "files while running:"; file_state "$d"
  if kill -0 "$pid" 2>/dev/null; then kill -TERM -- -"$pid" 2>/dev/null; echo "(sent SIGTERM to the session after ${secs}s; SIGINT is ignored by background jobs of a non-interactive shell)"; fi
  wait "$pid"; SCN_RC=$?
  kill "$wd" 2>/dev/null; wait "$wd" 2>/dev/null
  [ -n "${EXTRA_ENV:-}" ] && unset "${EXTRA_ENV%%=*}"
  echo "EXIT CODE of started command: $SCN_RC"
  echo "files after exit:"; file_state "$d"
  sleep 2; echo "processes left over in session 2 s after exit: $(pgrep -s "$pid" | wc -l)"
  case $form in
    staged-*|staged400-*|bare-*)
      sha256sum .runner .credentials .credentials_rsaparams 2>/dev/null | sed 's/^\(.\{12\}\).*  /\1...  /' > "$SRVDIR/sha.after"
      if cmp -s "$SRVDIR/sha.before" "$SRVDIR/sha.after"; then echo "config files byte-identical before/after"; else echo "config files CHANGED or removed:"; diff "$SRVDIR/sha.before" "$SRVDIR/sha.after"; fi ;;
  esac
  echo "runner stdout/stderr (first 12 lines):"; head -12 "$SRVDIR/out.txt" | cut -c1-200
  echo "requests seen by fake server ($(wc -l < "$REQLOG")):"; summarize_reqlog "$REQLOG" | awk '{c[$0]++} END{for(k in c) print c[k]"x  "k}' | sort -k2 | cut -c1-260
  echo "leak grep inside runner root (blob prefix, key prefix, sentinels; names only, excluding the 3 JIT files):"
  grep -rlF -e "$(head -c 60 "$SRVDIR/jitconfig.b64")" -e "$(head -c 40 "$SRVDIR/rsa.d.b64")" -e SENTINEL-CLIENT-ID "$d/_diag" "$d/_work" "$d"/.env "$d"/.path "$d"/run-helper.sh 2>/dev/null || echo "  (none)"
  echo "_diag files: $(ls "$d/_diag" 2>/dev/null | tr '\n' ' ')"
  cd "$HERE" || true
  SCN_OUT=$SRVDIR
  stop_server
  rm -rf "$d"
  return 0
}

# pristine_intact: prints how many files in the pristine tree are newer than the tarball (must be 0: nothing wrote through a hard link).
pristine_intact() {
  local n; n=$(find "$RUNNER" -newer "$M1/runner.tgz" 2>/dev/null | wc -l)
  echo "pristine runner tree: $n entries newer than the tarball (expected 0)"; [ "$n" = 0 ]
}
