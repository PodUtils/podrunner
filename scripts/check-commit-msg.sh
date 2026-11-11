#!/bin/sh
# Refuse a commit message that has an agent attribution line (AGENTS.md,
# Git): a `Co-Authored-By:` or `Claude-Session:` trailer, or a line with
# "Generated with". Lines that start with `#` are comments of git and are
# not read.
#
#   sh scripts/check-commit-msg.sh MESSAGE_FILE
#   sh scripts/check-commit-msg.sh --rev REV...    (the message of each commit)
#   sh scripts/check-commit-msg.sh --self-test     (planted messages)
#
# Exit 1 on an attribution line, 2 on a usage error.
set -u

bad() {
    grep -v '^#' | grep -n -i -E '^[[:space:]]*(co-authored-by|claude-session)[[:space:]]*:|generated with' | head -3
}

one() {
    found=$(bad)
    if [ -n "$found" ]; then
        echo "check-commit-msg: $1 has an agent attribution line (AGENTS.md, Git):" >&2
        printf '%s\n' "$found" >&2
        return 1
    fi
    return 0
}

case "${1:-}" in
    "") echo "usage: check-commit-msg.sh MESSAGE_FILE | --rev REV... | --self-test" >&2; exit 2 ;;
    --rev)
        shift
        rc=0
        for rev in "$@"; do
            git log -1 --format=%B "$rev" | one "commit $rev" || rc=1
        done
        exit "$rc"
        ;;
    --self-test)
        rc=0
        t() {
            if printf '%b' "$2" | one plant 2> /dev/null; then got=0; else got=1; fi
            if [ "$got" = "$1" ]; then echo "ok    $3"; else echo "FAIL  $3"; rc=1; fi
        }
        t 0 'record: a subject\n\nA body.\n' "a clean message passes"
        t 0 'record: a subject\n\n# Co-Authored-By: a comment of git\n' "a comment of git passes"
        t 1 'x: y\n\nCo-Authored-By: A <a@example.invalid>\n' "Co-Authored-By is refused"
        t 1 'x: y\n\nco-authored-by: A <a@example.invalid>\n' "co-authored-by in lower case is refused"
        t 1 'x: y\n\nClaude-Session: https://example.invalid/s\n' "Claude-Session is refused"
        t 1 'x: y\n\nGenerated with a tool\n' "Generated with is refused"
        exit "$rc"
        ;;
    *) [ -f "$1" ] || { echo "check-commit-msg: no file $1" >&2; exit 2; }
       one "the message" < "$1" ;;
esac
