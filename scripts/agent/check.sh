#!/bin/sh
# The checks of AGENTS.md before each push: fmt, clippy, rustdoc and the
# tests, the check of the record (cargo todo check), the size of the Rust
# files (scripts/check-size.py, with its self-test), the self-test of the
# summary of the user-path gate (scripts/user-gate-summary.py), the
# WebSocket attach of the user-path gate (scripts/ws-echo.py --self-test),
# the checks of the boot of hosted-boot.yml (scripts/hosted/boot-checks.py
# --self-test), the runner layer of the hosted snapshot
# (scripts/hosted/runner-layer.py --self-test), the other scripts of the
# hosted snapshot (their --self-test), the runner image release
# (scripts/runner-release.py --self-test), the images of a release on
# ghcr.io (scripts/ghcr.py --self-test), the commit messages
# (scripts/check-commit-msg.sh, with its self-test), the test of the
# installer (scripts/test-install.sh), the levels of proof of the newest
# notes of a release (scripts/check-notes.sh), the static build of
# x86_64 musl (scripts/check-static.sh), and that the tests leave no
# entry in their temp directory. Print each exit code and the
# first faults; exit 1 on a fault. With the argument notest, the tests are
# skipped. A full run (the tests and PODRUNNER_RUNNER_DIR) that passes
# writes the tree that it checked into .work/agent/checked-trees:
# .githooks/pre-push refuses a push of another tree.
#
#   sh scripts/agent/check.sh [notest]
set -u
cd "$(dirname "$0")/../.." || exit 1
log=${TMPDIR:-/tmp}/podrunner-check
mkdir -p "$log"
rc=0
# The tree of the working copy (tracked and new files), with no change to
# the index.
tree_now() {
    idx=$(mktemp) || return 1
    cp "$(git rev-parse --git-path index)" "$idx" &&
        GIT_INDEX_FILE=$idx git add -A >/dev/null 2>&1 &&
        GIT_INDEX_FILE=$idx git write-tree
    e=$?
    rm -f "$idx"
    return "$e"
}
tree=$(tree_now)
# Print each entry that the tests left in the directory $1; exit 1 if
# there is one.
tmp_left() {
    left=$(ls -A "$1") || return 1
    [ -z "$left" ] && return 0
    echo "the tests left these entries in their temp directory:"
    echo "$left" | head -20
    return 1
}
cargo fmt --all --check > "$log/fmt.log" 2>&1; f=$?
echo "fmt $f"; [ "$f" -eq 0 ] || { rc=1; head -20 "$log/fmt.log"; }
cargo clippy --workspace --all-targets --all-features -- -D warnings > "$log/clippy.log" 2>&1; c=$?
echo "clippy $c"; [ "$c" -eq 0 ] || { rc=1; grep -E "^(error|warning)" -A6 "$log/clippy.log" | head -40; }
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --workspace --all-features > "$log/doc.log" 2>&1; d=$?
echo "doc $d"; [ "$d" -eq 0 ] || { rc=1; grep -E "^(error|warning)" -A4 "$log/doc.log" | head -20; }
cargo run -q -p podrunner-todo -- check > "$log/record.log" 2>&1; r=$?
echo "record $r"; [ "$r" -eq 0 ] || { rc=1; head -20 "$log/record.log"; }
(cd fuzz && cargo fmt --check && cargo clippy -q --locked --all-targets -- -D warnings && cargo test -q --locked --lib) > "$log/fuzz.log" 2>&1; z=$?
echo "fuzz $z"; [ "$z" -eq 0 ] || { rc=1; tail -20 "$log/fuzz.log"; }
{ python3 -I scripts/check-size.py && python3 -I scripts/check-size.py --self-test; } > "$log/size.log" 2>&1; s=$?
echo "size $s"; [ "$s" -eq 0 ] || { rc=1; grep -v "^ok \|^check-size:" "$log/size.log" | head -20; }
python3 -I scripts/user-gate-summary.py --self-test > "$log/gate-summary.log" 2>&1; g=$?
echo "gate-summary $g"; [ "$g" -eq 0 ] || { rc=1; grep -v "^ok " "$log/gate-summary.log" | head -20; }
timeout 120 python3 -I scripts/ws-echo.py --self-test > "$log/ws.log" 2>&1; w=$?
echo "ws-echo $w"; [ "$w" -eq 0 ] || { rc=1; grep -v "^ok " "$log/ws.log" | head -20; }
python3 -I scripts/hosted/boot-checks.py --self-test > "$log/boot-checks.log" 2>&1; b=$?
echo "boot-checks $b"; [ "$b" -eq 0 ] || { rc=1; grep -v "^ok " "$log/boot-checks.log" | head -20; }
python3 -I scripts/hosted/runner-layer.py --self-test > "$log/runner-layer.log" 2>&1; rl=$?
echo "runner-layer $rl"; [ "$rl" -eq 0 ] || { rc=1; head -20 "$log/runner-layer.log"; }
# A subshell: its exit ends the group, not check.sh.
( for s in diff scrub plan c1 oci slice watch; do python3 -I "scripts/hosted/$s.py" --self-test || exit 1; done &&
    timeout 120 python3 -I scripts/hosted/push-blob.py --self-test ) > "$log/hosted.log" 2>&1; hs=$?
echo "hosted $hs"; [ "$hs" -eq 0 ] || { rc=1; tail -20 "$log/hosted.log"; }
python3 -I scripts/runner-release.py --self-test > "$log/runner-release.log" 2>&1; rr=$?
echo "runner-release $rr"; [ "$rr" -eq 0 ] || { rc=1; grep -v "^ok " "$log/runner-release.log" | head -20; }
timeout 120 python3 -I scripts/ghcr.py --self-test > "$log/ghcr.log" 2>&1; gc=$?
echo "ghcr $gc"; [ "$gc" -eq 0 ] || { rc=1; tail -20 "$log/ghcr.log"; }
{ sh scripts/check-commit-msg.sh --self-test && sh scripts/check-commit-msg.sh --rev HEAD; } > "$log/msg.log" 2>&1; m=$?
echo "commit-msg $m"; [ "$m" -eq 0 ] || { rc=1; grep -v "^ok" "$log/msg.log" | head -20; }
# The schema of podrunner.toml (T-479), where python3 has jsonschema; the
# gate runs it in its venv.
if python3 -I -c 'import jsonschema' 2> /dev/null; then
    python3 -I scripts/check-config-schema.py --self-test > "$log/config-schema.log" 2>&1; c=$?
    echo "config-schema $c"; [ "$c" -eq 0 ] || { rc=1; cat "$log/config-schema.log"; }
    # The design checks of the gate's job record (the word ceiling of
    # SPEC.md, the schemas, the contracts).
    { bash docs/design/verify.sh && sh scripts/check-design.sh --self-test && sh scripts/check-design.sh; } > "$log/design.log" 2>&1; d=$?
    echo "design $d"; [ "$d" -eq 0 ] || { rc=1; grep -v "^ok" "$log/design.log" | tail -20; }
else
    echo "config-schema and design skipped: no jsonschema module"
fi
sh scripts/test-install.sh > "$log/install.log" 2>&1; i=$?
echo "install $i"; [ "$i" -eq 0 ] || { rc=1; tail -20 "$log/install.log"; }
sh scripts/check-notes.sh --latest > "$log/notes.log" 2>&1; n=$?
echo "notes $n"; [ "$n" -eq 0 ] || { rc=1; cat "$log/notes.log"; }
t=x86_64-unknown-linux-musl
{ cargo build -q --release -p podrunner &&
    sh scripts/check-static.sh "target/$t/release/podrunner"; } > "$log/static.log" 2>&1; st=$?
echo "static $st"; [ "$st" -eq 0 ] || { rc=1; tail -20 "$log/static.log"; }
if [ "${1:-}" != notest ]; then
    # The tests get a temp directory of their own (T-591): other runs on
    # the host do not add to it, and each test removes what it makes.
    # No dot in the name: pod-unit reads a path after a dot as one word,
    # and a random suffix there looks like a secret (T-602).
    tmp=$(mktemp -d "${TMPDIR:-/tmp}/prt-XXXXXX") || exit 1
    TMPDIR=$tmp timeout 1800 cargo test --workspace --all-features > "$log/test.log" 2>&1; t=$?
    # Not "failed": each summary line has "0 failed" and would hide the cause.
    echo "test $t"; [ "$t" -eq 0 ] || { rc=1; grep -E "FAILED|panicked|^error|^failures:|^---- " "$log/test.log" | head -30; }
    tmp_left "$tmp"; l=$?
    echo "tmp-left $l"; [ "$l" -eq 0 ] || rc=1
    rm -rf "${tmp:?}"
    # The suites of level2.yml that need the official runner (T-256): with
    # PODRUNNER_RUNNER_DIR (an unpacked release of actions/runner), else a
    # line that says that they did not run.
    if [ -n "${PODRUNNER_RUNNER_DIR:-}" ]; then
        { timeout 1200 cargo test -q -p podrunner-broker-fake --test runner --test container -- --ignored --test-threads 1 &&
            timeout 1200 cargo test -q -p podrunner-local --test runner --test actions -- --ignored --test-threads 1 &&
            timeout 1800 cargo test -q -p podrunner --test local --test local_subdir --test local_proxy -- --ignored --test-threads 1 &&
            timeout 2400 cargo test -q -p podrunner --test parity -- --ignored --test-threads 1; } > "$log/runner.log" 2>&1; r=$?
        echo "runner $r"; [ "$r" -eq 0 ] || { rc=1; grep -E "FAILED|panicked|^error|^---- " "$log/runner.log" | head -30; }
    else
        echo "runner skipped: set PODRUNNER_RUNNER_DIR to an unpacked actions/runner (level2.yml runs these suites)"
    fi
fi
if [ "$rc" -eq 0 ] && [ "${1:-}" != notest ] && [ -n "${PODRUNNER_RUNNER_DIR:-}" ]; then
    if [ -z "$tree" ]; then
        echo "no tree of the working copy: not recorded"
    elif [ "$(tree_now)" = "$tree" ]; then
        mkdir -p .work/agent && echo "$tree" >> .work/agent/checked-trees
        echo "checked tree $tree"
    else
        echo "the tree changed during the checks: not recorded"
    fi
fi
exit "$rc"
