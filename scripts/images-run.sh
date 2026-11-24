#!/bin/sh
# SPDX-License-Identifier: 0BSD
# The runner-image.yml run whose image archives a release or the user-path
# gate takes (T-626). release.yml and user-gate.yml call this one script,
# so that the two rules cannot differ.
#
#   sh scripts/images-run.sh RUN    (RUN: the input images_run, can be empty)
#   sh scripts/images-run.sh --self-test
#
# The commit is HEAD of the current directory. GITHUB_REPOSITORY names the
# repository, and gh reads the API. With RUN, the run must be a green
# runner-image.yml run. A run of another commit serves only when images/ is
# the same at both commits, less the files that are not in the runner
# image. With no RUN, the green runner-image.yml run of HEAD.
#
# Out: the id of the run on stdout, each message on stderr. Exit 0 with a
# run, 1 when the rule refuses RUN, 2 on a usage error, 3 when RUN is empty
# and HEAD has no green run.
set -eu

# Not in the runner image: images/fake-githome.toml pins a test tool
# (T-509), images/pod-release.toml the tools of the index (T-562), and
# images/hosted/ the rules of the snapshot (T-539, T-542).
same_images() {
    git diff --quiet "$1" "$2" -- images/ ':(exclude)images/fake-githome.toml' \
        ':(exclude)images/pod-release.toml' ':(exclude)images/hosted'
}

say() { echo "images-run: $*" >&2; }

resolve() {
    run=$1
    [ -n "${GITHUB_REPOSITORY:-}" ] || { say "GITHUB_REPOSITORY is not set"; return 2; }
    sha=$(git rev-parse HEAD)
    if [ -z "$run" ]; then
        got=$(gh api "repos/$GITHUB_REPOSITORY/actions/workflows/runner-image.yml/runs?head_sha=$sha&status=success" \
            --jq '.workflow_runs[0].id // empty') || { say "the API did not give the runs of runner-image.yml"; return 1; }
        [ -n "$got" ] || { say "no green runner-image.yml run at $sha: dispatch it on this commit first, or give images_run"; return 3; }
        say "the green runner-image.yml run $got of $sha"
        echo "$got"
        return 0
    fi
    # A value that is not digits never goes into a path of the API.
    case $run in *[!0-9]*) say "images_run is the id of a run (digits only), not '$run'"; return 1 ;; esac
    got=$(gh api "repos/$GITHUB_REPOSITORY/actions/runs/$run" --jq '"\(.path) \(.head_sha) \(.conclusion)"') ||
        { say "the API did not give run $run"; return 1; }
    # shellcheck disable=SC2086 # three words: path, sha, conclusion
    set -- $got
    if [ "$#" -ne 3 ] || [ "$1 $3" != ".github/workflows/runner-image.yml success" ]; then
        say "run $run is not a green runner-image.yml run: $got"; return 1
    fi
    case $2 in *[!0-9a-f]* | "") say "run $run has no commit: $got"; return 1 ;; esac
    if [ "$2" != "$sha" ]; then
        # A shallow checkout has no older commit: fetch it.
        git cat-file -e "$2^{commit}" 2> /dev/null || git fetch -q --depth=200 origin "$2" 2> /dev/null ||
            { say "run $run is of $2, which this checkout cannot fetch"; return 1; }
        same_images "$2" "$sha" ||
            { say "run $run is of $2, and images/ changed since: run runner-image.yml on $sha"; return 1; }
        say "run $run is of $2; images/ is the same at $sha"
    else
        say "run $run is of $sha"
    fi
    echo "$run"
}

self_test() {
    t=$(mktemp -d)
    trap 'rm -rf "$t"' EXIT
    here=$(cd "$(dirname "$0")" && pwd)/$(basename "$0")
    # A fake gh: the JSON of a path from $t/api, filtered by jq as gh --jq.
    mkdir -p "$t/bin" "$t/api" "$t/repo"
    cat > "$t/bin/gh" <<'EOF'
#!/bin/sh
[ "$1" = api ] && [ "$3" = --jq ] || { echo "fake gh: $*" >&2; exit 2; }
f=$FAKE_API/$(printf '%s' "$2" | tr '/?&=' '____').json
[ -f "$f" ] || { echo "gh: Not Found (HTTP 404)" >&2; exit 1; }
exec jq -r "$4" "$f"
EOF
    chmod 0755 "$t/bin/gh"
    r=example/repo
    api() { printf '%s' "$2" > "$t/api/$(printf '%s' "repos/$r/$1" | tr '/?&=' '____').json"; }
    g() { git -C "$t/repo" -c user.name=t -c user.email=t@example.invalid "$@"; }
    g init -q
    mkdir -p "$t/repo/images/runner" "$t/repo/images/hosted" "$t/repo/docs"
    echo a > "$t/repo/images/runner/runner.toml"
    echo a > "$t/repo/docs/a.md"
    g add -A && g commit -q -m one
    one=$(g rev-parse HEAD)
    echo b > "$t/repo/docs/a.md"
    echo b > "$t/repo/images/fake-githome.toml"
    echo b > "$t/repo/images/hosted/rules.toml"
    g add -A && g commit -q -m two
    two=$(g rev-parse HEAD)
    echo c > "$t/repo/images/runner/runner.toml"
    g add -A && g commit -q -m three
    three=$(g rev-parse HEAD)
    run_json() { printf '{"path":".github/workflows/%s","head_sha":"%s","conclusion":"%s"}' "$1" "$2" "$3"; }
    api actions/runs/11 "$(run_json runner-image.yml "$one" success)"
    api actions/runs/12 "$(run_json runner-image.yml "$two" failure)"
    api actions/runs/13 "$(run_json user-gate.yml "$two" success)"
    api actions/runs/14 "$(run_json runner-image.yml "$two" success)"
    api actions/runs/15 "$(run_json runner-image.yml 0123456789abcdef0123456789abcdef01234567 success)"
    api actions/runs/16 "$(run_json runner-image.yml "$three" null)"
    api "actions/workflows/runner-image.yml/runs?head_sha=$two&status=success" '{"workflow_runs":[{"id":14}]}'
    api "actions/workflows/runner-image.yml/runs?head_sha=$three&status=success" '{"workflow_runs":[]}'
    rc=0
    # case WANT_EXIT WANT_OUT AT RUN NAME
    case_() {
        set +e
        out=$(cd "$t/repo" && git checkout -q "$3" && PATH="$t/bin:$PATH" FAKE_API=$t/api GITHUB_REPOSITORY=$r \
            sh "$here" "$4" 2> "$t/err")
        got=$?
        set -e
        if [ "$got" = "$1" ] && [ "$out" = "$2" ]; then
            echo "ok    $5"
        else
            echo "FAIL  $5: exit $got (want $1), out '$out' (want '$2')"
            sed 's/^/  | /' "$t/err"
            rc=1
        fi
    }
    case_ 0 11 "$one" 11 "a green run of the same commit"
    case_ 0 11 "$two" 11 "a green run of an older commit: only docs/ and the excluded files changed"
    case_ 1 "" "$three" 11 "a green run of an older commit: images/runner/runner.toml changed"
    case_ 1 "" "$two" 12 "a red run"
    case_ 1 "" "$three" 16 "a run with no conclusion"
    case_ 1 "" "$two" 13 "a green run of another workflow"
    case_ 1 "" "$two" 15 "a run of a commit that the checkout cannot fetch"
    case_ 1 "" "$two" 99 "a run that the API does not know"
    case_ 1 "" "$two" "1 2" "a value with a space"
    # shellcheck disable=SC2016 # the text of an expression, not expanded
    case_ 1 "" "$two" '${{ github.run_id }}' "an expression in place of a value"
    case_ 1 "" "$two" ../11 "a path in place of a value"
    case_ 0 14 "$two" "" "no value: the green run of the commit"
    case_ 3 "" "$three" "" "no value, and no green run of the commit"
    [ "$rc" = 0 ] && echo "images-run: self-test passed" || echo "images-run: self-test failed"
    return "$rc"
}

case ${1-} in
    --self-test) [ "$#" = 1 ] || { say "usage: images-run.sh RUN | --self-test"; exit 2; }; self_test ;;
    *) [ "$#" = 1 ] || { say "usage: images-run.sh RUN | --self-test"; exit 2; }; resolve "$1" ;;
esac
