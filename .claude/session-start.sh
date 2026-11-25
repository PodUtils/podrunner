#!/bin/sh
# SPDX-License-Identifier: 0BSD
root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P) || exit 1
if ! sh "$root/scripts/session-start.sh"; then
    echo "session-start: see docs/maintenance.md and run sh scripts/session-start.sh again" >&2
    exit 1
fi
