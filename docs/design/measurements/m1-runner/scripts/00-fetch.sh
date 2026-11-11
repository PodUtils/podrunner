#!/bin/bash
# Q0: fetch and verify the pinned official runner (linux-x64 v2.338.2) and its source tag.
# Pinned: tarball sha256 baebf204...7c40 (release page digest AND release body agree, observed 2026-11-10).
# Source archive is only used to read code; its sha is recorded, not enforced (GitHub may regenerate archives).
. "$(dirname "$0")/lib.sh"
start_log 00-fetch
conditions
mkdir -p "$M1"; cd "$M1" || exit 2
if [ ! -f runner.tgz ] || ! echo "$TARBALL_SHA256  runner.tgz" | sha256sum -c - >/dev/null 2>&1; then
  curl -fL --max-time 600 -o runner.tgz "$TARBALL_URL"; rc=$?
  if [ $rc -ne 0 ]; then
    echo "direct download failed rc=$rc, trying fallback"
    curl -fL --max-time 900 -o runner.tgz "$TARBALL_FALLBACK_URL" || { echo "both downloads failed"; exit 2; }
  fi
fi
echo "$TARBALL_SHA256  runner.tgz" | sha256sum -c - || { echo "sha256 MISMATCH"; exit 1; }
if [ ! -x r/bin/Runner.Listener ]; then rm -rf r; mkdir r; tar xzf runner.tgz -C r || exit 2; fi
if [ ! -d src/src ]; then
  curl -fL --max-time 300 -o src.tgz "$SRC_URL" || exit 2
  rm -rf src; mkdir src; tar xzf src.tgz -C src --strip-components=1 || exit 2
fi
if [ ! -d hooks/packages/hooklib ]; then
  curl -fL --max-time 300 -o hooks.tgz "$HOOKS_URL" || exit 2
  echo "$HOOKS_SHA256  hooks.tgz" | sed "s/ *#.*//" | sha256sum -c - || { echo "hooks archive sha256 MISMATCH"; exit 1; }
  rm -rf hooks; mkdir hooks; tar xzf hooks.tgz -C hooks --strip-components=1 || exit 2
fi
echo "source archive sha256: $(sha256sum src.tgz | cut -d' ' -f1)"
echo "releaseVersion file: $(cat src/src/runnerversion 2>/dev/null)"
ls r | head
exit 0
