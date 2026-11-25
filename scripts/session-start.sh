#!/bin/sh
# SPDX-License-Identifier: 0BSD
# Set up Git and start the watchers in the lead checkout.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

git config --local user.name Azathothas
git config --local user.email ajamx101@gmail.com
git config --local core.hooksPath .githooks
git config --local commit.gpgsign false

if [ "$(git rev-parse --absolute-git-dir)" != "$(git rev-parse --path-format=absolute --git-common-dir)" ]; then
    echo "session-start: the lead checkout owns the watchers"
    exit 0
fi

sync_main() {
    tree_status=$(git status --porcelain) || return 1
    if [ -n "$tree_status" ]; then
        return 0
    fi
    upstream=refs/remotes/origin/main
    if ! GIT_TERMINAL_PROMPT=0 timeout 60 git fetch -q origin "+refs/heads/main:$upstream" </dev/null; then
        echo "session-start: could not fetch origin main" >&2
        return 1
    fi
    if [ "$(git symbolic-ref --short -q HEAD || :)" != main ]; then
        if ! git merge-base --is-ancestor HEAD "$upstream"; then
            echo "session-start: keep the checkout with local commits or incomplete history" >&2
            return 1
        fi
        if git show-ref --verify --quiet refs/heads/main; then
            git switch --quiet main || return 1
        else
            git switch --quiet --track -c main origin/main || return 1
        fi
    fi
    if ! git merge --quiet --ff-only "$upstream"; then
        echo "session-start: local main could not fast-forward" >&2
        return 1
    fi
}

umask 077
pin=5fd6fb7e3495d5e9a591e36c5752e268309f6e4a
watcher_dir=.work/podcrates-agent/$pin
mkdir -p "$watcher_dir"
exec 9>.work/session-start.lock
flock -w 30 9

fetch_watcher() (
    file=$1
    sum=$2
    cd "$watcher_dir"
    if printf '%s  %s\n' "$sum" "$file" | sha256sum --check --status 2>/dev/null; then
        exit 0
    fi
    stage=$(mktemp "$file.XXXXXXXX")
    trap 'rm -f -- "$stage"' EXIT
    url=https://raw.githubusercontent.com/PodUtils/podcrates/$pin/scripts/agent/$file
    curl -qfsSL --proto '=https' --proto-redir '=https' --max-time 30 --retry 2 -o "$stage" "$url" </dev/null
    if ! printf '%s  %s\n' "$sum" "$stage" | sha256sum --check --status; then
        printf 'session-start: %s failed the SHA-256 check\n' "$file" >&2
        exit 1
    fi
    mv -f -- "$stage" "$file"
)

status=0
sync_main || status=1
fetch_watcher watch.py 14702c073ba50d68572531f29dbbc10b366b25216ef69658da385ac60d4e9391
fetch_watcher watch_test.py ff8d8a3d6ebbbde57135e174ee93b0330c237f611f479451f76e1173280a7568
fetch_watcher runs.py 80754a64ba6281d7b0e03d59932bf6a7fe93aaab956bda907315854d81d13d36
timeout 20 python3 -I "$watcher_dir/watch.py" --self-test </dev/null >/dev/null
timeout 20 python3 -I "$watcher_dir/runs.py" --self-test </dev/null >/dev/null

start_watcher() {
    name=$1
    shift
    log=.work/$name.log
    lock=.work/$name.lock
    touch "$lock" || return 1
    if flock -n "$lock" true; then
        nohup flock -F "$lock" python3 -I "$watcher_dir/$name.py" "$@" \
            >>"$log" 2>&1 </dev/null 9>&- &
        for _attempt in 1 2 3 4 5; do
            flock -n "$lock" true || break
            sleep 1
        done
    fi
    if flock -n "$lock" true; then
        printf 'session-start: %s did not start. See %s\n' "$name" "$log" >&2
        return 1
    fi
    printf 'session-start: %s runs. Log %s\n' "$name" "$log"
}

start_watcher watch podrunner --state .work/agent || status=1
start_watcher runs --repo PodUtils/podrunner --branch main --every 120 || status=1
exit "$status"
