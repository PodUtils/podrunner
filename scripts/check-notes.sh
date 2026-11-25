#!/bin/sh
# SPDX-License-Identifier: 0BSD
# The notes of a release name the level of the proof of each claim
# (AGENTS.md, section 12; T-247): the three sections below must be there,
# and "What is not proven" must not be empty. A reader must not take a
# claim of CI for a claim of a user host.
#
# A release after the base of CHANGES.md also needs its section there.
#
#   sh scripts/check-notes.sh NOTES
#   sh scripts/check-notes.sh --latest   the notes of the newest version;
#                                        rows under "## next" are the
#                                        changes after it, not a fault
#   sh scripts/check-notes.sh --stamp    stamp the contract files in CHANGES.md
set -eu
die() {
    echo "check-notes: $*" >&2
    exit 1
}
[ $# -eq 1 ] || die "usage: sh scripts/check-notes.sh NOTES | --latest | --stamp"
notes=$1
root=$(cd "$(dirname "$0")/.." && pwd)
changes=${CHECK_NOTES_CHANGES:-$root/CHANGES.md}
# The SHA-256 of each contract file (T-510): a change of one needs a row
# of CHANGES.md and a new stamp (--stamp writes it).
stamp() {
    (cd "${CHECK_NOTES_ROOT:-$root}" && sha256sum docs/contracts/*.md schemas/*.json | awk '{ printf "<!-- contract %s %s -->\n", $2, $1 }')
}
if [ "$notes" = --stamp ]; then
    tmp=$changes.tmp
    grep -v '^<!-- contract ' "$changes" > "$tmp"
    stamp >> "$tmp"
    mv "$tmp" "$changes"
    echo "check-notes: $changes: the contracts are stamped"
    exit 0
fi
if [ "$(grep '^<!-- contract ' "$changes")" != "$(stamp)" ]; then
    die "$changes: a contract file changed (docs/contracts, schemas): add its row under \"## next\", then run sh scripts/check-notes.sh --stamp"
fi
latest=
if [ "$notes" = --latest ]; then
    latest=1
    root=$(cd "$(dirname "$0")/.." && pwd)
    notes=$(printf '%s\n' "$root"/docs/releases/v*.md | sort -V | tail -n 1)
fi
[ -f "$notes" ] || die "$notes: no such file"
for h in "## What is proven in CI" "## What is proven on a constrained host as a user" "## What is not proven"; do
    grep -qx "$h" "$notes" || die "$notes: no section \"$h\": name the level of the proof of each claim"
done
# The lines of "What is not proven" up to the next section.
open=$(sed -n '/^## What is not proven$/,/^## /p' "$notes" | sed '1d;/^## /d' | grep -c '[^[:space:]]' || true)
[ "$open" -gt 0 ] || die "$notes: the section \"What is not proven\" is empty: write what no run proved"
# CHANGES.md (T-510): the notes of a release after the base of the file
# need a section of their tag, and no row left under "## next" (the check
# of the notes of a release before its tag; with --latest the release is
# out, and its rows under "## next" are the changes after it).
version=$(basename "$notes" .md)
# shellcheck disable=SC2016 # the backticks are Markdown.
base=$(sed -n 's/.*The base is `\(v[^`]*\)`.*/\1/p' "$changes" | head -n 1)
[ -n "$base" ] || die "$changes: no base version"
if [ "$version" != "$base" ] && [ "$(printf '%s\n%s\n' "$base" "$version" | sort -V | tail -n 1)" = "$version" ]; then
    grep -qx "## $version" "$changes" || die "$changes: no section \"## $version\": rename \"## next\" to the tag of the release"
    [ -n "$latest" ] && left=0 || left=$(sed -n '/^## next$/,/^## /p' "$changes" | grep -c '^| [^-]' || true)
    [ "$left" -le 1 ] || die "$changes: rows are left under \"## next\": move them to \"## $version\""
fi
echo "check-notes: $notes names the level of its proof"
