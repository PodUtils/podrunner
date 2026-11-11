#!/bin/bash
# Q0: build the podvm binary the way the project documents (podvm-gate, then podvm-dev build) and time it.
# Inputs: podvm clone at commit 36b15ec034f5 (PODVM_REPO, required),
#         toolchain from the clone's rust-toolchain.toml (installed with rustup into $WORK, never system-wide).
# Exit: 0 built, 1 built but wrong commit, 2 could not build.
. "$(dirname "$0")/lib.sh"
REPO=${PODVM_REPO:?set PODVM_REPO to a clone of podvm}
PIN=36b15ec034f5
[ -d "$REPO/.git" ] || { echo "no clone at $REPO"; exit 2; }
echo "date=$(date -u +%FT%TZ) kernel=$(uname -r) cpus=$(nproc) virt=$(systemd-detect-virt)"
rm -rf "$SRC"; cp -a "$REPO" "$SRC"
cd "$SRC" || exit 2
HEAD=$(git rev-parse --short=12 HEAD); echo "commit=$HEAD"
CH=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' rust-toolchain.toml); echo "pinned channel=$CH"
export RUSTUP_HOME=$WORK/rustup CARGO_HOME=$WORK/cargo
export PATH=$CARGO_HOME/bin:$PATH
if ! command -v rustup >/dev/null; then
  command -v rustup >/dev/null || { for p in /usr/local/cargo/bin /usr/local/bin; do [ -x $p/rustup ] && PATH=$p:$PATH; done; }
fi
command -v rustup >/dev/null || { echo "no rustup"; exit 2; }
command -v zig >/dev/null || { echo "no zig on PATH (the build compiles ring with zig cc)"; exit 2; }
echo "zig=$(zig version)"
T0=$(date +%s.%N)
rustup toolchain install "$CH" --profile minimal -t x86_64-unknown-linux-musl || exit 2
T1=$(date +%s.%N)
cargo build --release -p podvm-gate --target x86_64-unknown-linux-gnu || exit 2
cargo build --release -p podvm-gate --bin podvm-interpose-build || exit 2
T2=$(date +%s.%N)
./target/x86_64-unknown-linux-gnu/release/podvm-dev build || exit 2
T3=$(date +%s.%N)
awk -v a="$T0" -v b="$T1" -v c="$T2" -v d="$T3" 'BEGIN{printf "wall toolchain-install=%.1fs gate-tools=%.1fs podvm-dev-build=%.1fs total=%.1fs (1 run, cold CARGO_HOME)\n", b-a, c-b, d-c, d-a}'
rustc --version
ls -l "$PODVM"; "$PODVM" --version
[ "$HEAD" = "$PIN" ] || { echo "WRONG COMMIT: $HEAD != $PIN"; exit 1; }
exit 0
