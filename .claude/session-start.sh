#!/bin/sh
# SPDX-License-Identifier: 0BSD
root=${CLAUDE_PROJECT_DIR:-"$(dirname -- "$0")/.."}
root=$(CDPATH='' cd -- "$root" && pwd -P) || exit 1
if ! sh "$root/scripts/session-start.sh"; then
    echo "session-start: see docs/cold-start.md and run sh scripts/session-start.sh again" >&2
fi
if [ "${CLAUDE_CODE_REMOTE:-}" = true ] && [ -n "${CLAUDE_ENV_FILE:-}" ]; then
    quoted_root=$(printf '%s' "$root" | sed "s/'/'\\\\''/g")
    {
        printf "podrunner_project_directory='%s'\n" "$quoted_root"
        cat <<'ENV'
podrunner_previous_directory=$PWD
if cd -- "$podrunner_project_directory"; then
    . ./scripts/cloud-env.sh
    cd -- "$podrunner_previous_directory"
fi
unset podrunner_project_directory podrunner_previous_directory
ENV
    } >> "$CLAUDE_ENV_FILE"
fi
