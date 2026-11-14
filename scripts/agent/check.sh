#!/bin/sh
# The checks of AGENTS.md before each push: fmt, clippy, rustdoc and the
# tests, the check of the record (cargo todo check), the size of the Rust
# files (scripts/check-size.py, with its self-test), the commit messages
# (scripts/check-commit-msg.sh, with its self-test), and the static build
# of x86_64 musl (scripts/check-static.sh). Print each exit code and the
# first faults; exit 1 on a fault. With the argument notest, the tests are
# skipped.
#
#   sh scripts/agent/check.sh [notest]
set -u
cd "$(dirname "$0")/../.." || exit 1
log=${TMPDIR:-/tmp}/podrunner-check
mkdir -p "$log"
rc=0
cargo fmt --all --check > "$log/fmt.log" 2>&1; f=$?
echo "fmt $f"; [ "$f" -eq 0 ] || { rc=1; head -20 "$log/fmt.log"; }
cargo clippy --workspace --all-targets --all-features -- -D warnings > "$log/clippy.log" 2>&1; c=$?
echo "clippy $c"; [ "$c" -eq 0 ] || { rc=1; grep -E "^(error|warning)" -A6 "$log/clippy.log" | head -40; }
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --workspace --all-features > "$log/doc.log" 2>&1; d=$?
echo "doc $d"; [ "$d" -eq 0 ] || { rc=1; grep -E "^(error|warning)" -A4 "$log/doc.log" | head -20; }
cargo run -q -p podrunner-todo -- check > "$log/record.log" 2>&1; r=$?
echo "record $r"; [ "$r" -eq 0 ] || { rc=1; head -20 "$log/record.log"; }
(cd fuzz && cargo fmt --check && cargo clippy -q --all-targets -- -D warnings && cargo test -q --lib) > "$log/fuzz.log" 2>&1; z=$?
echo "fuzz $z"; [ "$z" -eq 0 ] || { rc=1; tail -20 "$log/fuzz.log"; }
{ python3 -I scripts/check-size.py && python3 -I scripts/check-size.py --self-test; } > "$log/size.log" 2>&1; s=$?
echo "size $s"; [ "$s" -eq 0 ] || { rc=1; grep -v "^ok \|^check-size:" "$log/size.log" | head -20; }
{ sh scripts/check-commit-msg.sh --self-test && sh scripts/check-commit-msg.sh --rev HEAD; } > "$log/msg.log" 2>&1; m=$?
echo "commit-msg $m"; [ "$m" -eq 0 ] || { rc=1; grep -v "^ok" "$log/msg.log" | head -20; }
t=x86_64-unknown-linux-musl
{ cargo build -q --release -p podrunner &&
    sh scripts/check-static.sh "target/$t/release/podrunner"; } > "$log/static.log" 2>&1; st=$?
echo "static $st"; [ "$st" -eq 0 ] || { rc=1; tail -20 "$log/static.log"; }
if [ "${1:-}" != notest ]; then
    timeout 1800 cargo test --workspace --all-features > "$log/test.log" 2>&1; t=$?
    # Not "failed": each summary line has "0 failed" and would hide the cause.
    echo "test $t"; [ "$t" -eq 0 ] || { rc=1; grep -E "FAILED|panicked|^error|^failures:|^---- " "$log/test.log" | head -30; }
fi
exit "$rc"
