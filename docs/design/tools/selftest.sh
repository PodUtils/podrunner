#!/usr/bin/env bash
# Planted-defect tests of the guards. For each guard: a clean copy must pass (exit 0),
# and a copy with one planted defect must fail (exit 1). A guard that fails on the
# clean copy, or passes on a defect, makes this script fail.
#
# Every plant is made in a full copy of the tree under a temporary directory. The
# real tree is never written: the script lists it with checksums before and after,
# and fails if the two differ.
# Exit 0 all as expected, 1 a guard did not behave or the real tree changed, 2 could not run.
set -u
export PYTHONDONTWRITEBYTECODE=1
here="$(cd "$(dirname "$0")/.." && pwd)" || exit 2
work="$(mktemp -d)" || exit 2
trap 'rm -rf "$work"' EXIT
bad=0
n=0

snapshot() { # a sorted listing of the real tree with a checksum for every file
  ( cd "$here" && find . -mindepth 1 -path ./.work -prune -o \( -type f -o -type l -o -type d \) -print \
      | LC_ALL=C sort | while IFS= read -r p; do
          if [ -f "$p" ] && [ ! -L "$p" ]; then printf '%s %s\n' "$p" "$(sha256sum < "$p" | cut -d' ' -f1)"; else printf '%s\n' "$p"; fi
        done )
}
before="$(snapshot)"

if [ -n "$(find "$here" -path "$here/.work" -prune -o -type l -print | head -n 1)" ]; then
  echo "selftest: the tree holds a symbolic link, and a plant could write through it" >&2
  exit 2
fi

fresh() { # fresh DIR : a full copy of the tree, no links back to it
  rm -rf "$1"; mkdir -p "$1"
  tar -C "$here" --exclude=./.work -cf - . | tar -C "$1" -xf -
}

run() { # run DIR TOOL : quiet, returns the tool's own exit code
  (cd "$1" && python3 "tools/$2.py" >/dev/null 2>&1)
  return $?
}

expect() { # expect WANT DIR TOOL LABEL
  local want=$1 dir=$2 tool=$3 label=$4
  n=$((n + 1))
  run "$dir" "$tool"; local got=$?
  if [ "$got" -ne "$want" ]; then
    bad=$((bad + 1)); echo "selftest FAIL: $label: $tool exit $got, wanted $want"
  fi
}

jsonedit() { # jsonedit FILE PYTHON-STATEMENTS on the loaded object o
  python3 - "$1" "$2" <<'PY'
import json, sys
path, code = sys.argv[1], sys.argv[2]
o = json.load(open(path))
exec(code)
json.dump(o, open(path, "w"))
PY
}

# 1. The clean copy passes every guard.
c="$work/clean"; fresh "$c"
for t in check_schemas check_contracts check_fields check_codes check_tiers check_docs; do expect 0 "$c" "$t" "clean copy"; done

# 2. check_docs defects. Strings that the guard looks for are built from pieces,
#    so that this file does not trip the guard itself.
emdash=$(printf '\xe2\x80\x94'); endash=$(printf '\xe2\x80\x93')
dd2=$(printf ' %s%s ' - -); absroot="/work""space/x"
d="$work/d1"; fresh "$d"; printf '\nA long dash %s here.\n' "$emdash" >> "$d/SPEC.md"; expect 1 "$d" check_docs "em dash in SPEC"
d="$work/d2"; fresh "$d"; printf '\nSee §99.9 for it.\n' >> "$d/SPEC.md"; expect 1 "$d" check_docs "dead section number"
d="$work/d3"; fresh "$d"; printf '\nSee `recon/NOPE.md` for it.\n' >> "$d/SPEC.md"; expect 1 "$d" check_docs "dead file pointer"
d="$work/d4"; fresh "$d"; printf '\nRun it in %s now.\n' "$absroot" >> "$d/SPEC.md"; expect 1 "$d" check_docs "absolute path"
d="$work/d5"; fresh "$d"; printf '\nSee SPEC §77 here.\n' >> "$d/contracts/hook.md"; expect 1 "$d" check_docs "dead section number in a contract"
d="$work/d6"; fresh "$d"; sed -i 's/Revision: draft 5\./Revision: draft 3./' "$d/SPEC.md"; expect 1 "$d" check_docs "revision line"
d="$work/d9"; fresh "$d"; printf '\n# %s\n' "$absroot" >> "$d/tools/check_tiers.py"; expect 1 "$d" check_docs "absolute path in a python tool"
d="$work/d10"; fresh "$d"; printf '\necho a%sb\n' "$dd2" >> "$d/verify.sh"; expect 1 "$d" check_docs "spaced double hyphen in a shell script"
d="$work/d11"; fresh "$d"; printf '\nA long dash %s here.\n' "$endash" >> "$d/recon/R1-podvm.md"; expect 1 "$d" check_docs "en dash in recon/"
d="$work/d12"; fresh "$d"; printf '\nSee WP-99 for it.\n' >> "$d/SPEC.md"; expect 1 "$d" check_docs "undefined work package"
d="$work/d13"; fresh "$d"; sed -i '/^| WP-06 |/d' "$d/SPEC.md"; expect 1 "$d" check_docs "a hole in the work package numbers"
d="$work/d14"; fresh "$d"; printf '\nSee U99 for it.\n' >> "$d/SPEC.md"; expect 1 "$d" check_docs "undefined unknown id"
d="$work/d15"; fresh "$d"; sed -i 's/^SPEC_WORD_CEILING = .*/SPEC_WORD_CEILING = 100/' "$d/tools/check_docs.py"; expect 1 "$d" check_docs "SPEC longer than its word ceiling"
d="$work/d16"; fresh "$d"; printf 'WORK=${TMPDIR:-/tmp}/x\n' > "$d/tools/default_ok.sh"; expect 0 "$d" check_docs "a TMPDIR default is allowed"
d="$work/d17"; fresh "$d"; printf '\nSee WP-05c for it.\n' >> "$d/SPEC.md"; expect 1 "$d" check_docs "an undefined work package suffix"
d="$work/d18"; fresh "$d"; printf '\nSee WP-05a and WP-05 here.\n' >> "$d/SPEC.md"; expect 0 "$d" check_docs "a defined suffixed work package and its base number"


# 3. check_schemas defects.
d="$work/s1"; fresh "$d"
cp "$d/examples/valid/job-offer.trusted-push.json" "$d/examples/invalid/job-offer.accepted.json"
jsonedit "$d/examples/invalid/EXPECT.json" 'o["job-offer.accepted.json"] = "tier"'
expect 1 "$d" check_schemas "an invalid example that validates"
d="$work/s2"; fresh "$d"
jsonedit "$d/examples/invalid/EXPECT.json" 'o["job-offer.age-zero.json"] = "payload/argv"'
expect 1 "$d" check_schemas "an invalid example rejected for the wrong reason"
d="$work/s3"; fresh "$d"
jsonedit "$d/examples/valid/job-offer.fork-pr.json" 'o["policy"]["allow"][0]["verb"] = "x"'
expect 1 "$d" check_schemas "a valid offer whose policy fails the broker schema"
d="$work/s4"; fresh "$d"; echo '{}' > "$d/examples/invalid/EXPECT.json"; expect 1 "$d" check_schemas "invalid examples without an expected reason"
d="$work/s5"; fresh "$d"
jsonedit "$d/schemas/run-report-public.v1.schema.json" 'o["additionalProperties"] = True'
expect 1 "$d" check_schemas "a public report schema that lets the host through"

d="$work/s6"; fresh "$d"
jsonedit "$d/schemas/job-offer.v1.schema.json" 'del o["if"]; del o["then"]'
expect 1 "$d" check_schemas "a payload on a github-scaleset offer that the schema lets through"
d="$work/s7"; fresh "$d"
cp "$d/examples/valid/job-offer.fake-container.json" "$d/examples/valid/job-offer.fake-two.json"
jsonedit "$d/examples/valid/job-offer.fake-two.json" 'o["source"] = "github-scaleset/org-a-x"'
expect 1 "$d" check_schemas "a valid offer from a github-scaleset source that carries a payload"

# 3b. check_contracts defects.
d="$work/k1"; fresh "$d"; sed -i '0,/"op":"probe","id":"1"}/s//"op":"probe","id":"1"/' "$d/contracts/unit-driver.md"; expect 1 "$d" check_contracts "a contract message that is not JSON"
d="$work/k2"; fresh "$d"; sed -i '0,/"op":"destroy"/s//"op":"destory"/' "$d/contracts/unit-driver.md"; expect 1 "$d" check_contracts "a request whose op has no heading"
d="$work/k3"; fresh "$d"; sed -i 's/^### withdraw$/### withdrawn/' "$d/contracts/source-driver.md"; expect 1 "$d" check_contracts "an operation heading renamed"
d="$work/k4"; fresh "$d"; jsonedit "$d/examples/valid/unit-capability.podvm-userland.json" 'del o["principal"]'
expect 1 "$d" check_contracts "a nested capability record that fails its schema"
d="$work/k5"; fresh "$d"; sed -i 's/"kind":"vsock"/"kind":"tcp"/' "$d/contracts/unit-driver.md"; expect 1 "$d" check_contracts "an egress form that is not defined"
d="$work/k6"; fresh "$d"; sed -i 's/"kind":"tcp-loopback","host":"127.0.0.1","port":3128/"kind":"tcp-loopback","host":"127.0.0.1"/' "$d/contracts/unit-driver.md"; expect 1 "$d" check_contracts "an egress form with a missing field"

d="$work/k7"; fresh "$d"; sed -i 's/"type":"started"/"type":"begun"/g' "$d/contracts/source-driver.md"; expect 1 "$d" check_contracts "an event type of next with no message shape"

d="$work/k8"; fresh "$d"; sed -i 's/`\/github\/workflow`/`\/github\/wf`/' "$d/contracts/hook.md"; expect 1 "$d" check_contracts "a mount target that the two contracts do not share"
d="$work/k9"; fresh "$d"; sed -i 's/^| `\/__e` | `externals` | ro |$/| `\/__e` | `externals` | ro |\n| `\/__x` | `extra` | rw |/' "$d/contracts/unit-driver.md"; expect 1 "$d" check_contracts "a mount table entry that hook.md does not name"

d="$work/k10"; fresh "$d"; sed -i 's/^| `\/__e` | `externals` | ro |$/| `\/__e` | `externals` | rw |/' "$d/contracts/unit-driver.md"; expect 1 "$d" check_contracts "the read-only mount /__e listed as rw"
d="$work/k11"; fresh "$d"; sed -i '0,/"registries":\["ghcr.io"\],/s///' "$d/contracts/unit-driver.md"; expect 1 "$d" check_contracts "a start example without registries"
d="$work/k12"; fresh "$d"; sed -i 's/`\/github\/file_commands`, //' "$d/contracts/hook.md"; expect 1 "$d" check_contracts "a container action mount that hook.md does not name"
d="$work/k13"; fresh "$d"; sed -i 's/^| `\/__w\/_tool` |/| `\/__t` |/' "$d/contracts/unit-driver.md"; expect 1 "$d" check_contracts "the tool cache row of the job table named /__t, not /__w/_tool"
d="$work/k14"; fresh "$d"; sed -i '/^| `\/github\/workspace` |/d' "$d/contracts/unit-driver.md"; expect 1 "$d" check_contracts "a row dropped from the container action table"
d="$work/k15"; fresh "$d"; sed -i 's/^| `\/github\/workspace` | the workspace directory/| `\/github\/workspace` | the work directory, the workspace directory/' "$d/contracts/unit-driver.md"; expect 0 "$d" check_contracts "the source text of an action table row reworded"

# 3c. verify.sh names a missing jsonschema module and stops before any check (and before this self-test, so it cannot recurse).
d="$work/v1"; fresh "$d"; mkdir "$d/.fake"; printf 'raise ImportError("planted")\n' > "$d/.fake/jsonschema.py"
n=$((n + 1)); (cd "$d" && PYTHONPATH="$d/.fake" bash verify.sh >"$d/v1.out" 2>&1); got=$?
if [ "$got" -ne 2 ] || ! grep -q 'needs python3 with the jsonschema module' "$d/v1.out"; then
  bad=$((bad + 1)); echo "selftest FAIL: verify.sh with a missing jsonschema: exit $got, wanted 2 and the message"
fi

# 4. check_fields defect: a schema property that no document names.
d="$work/f1"; fresh "$d"
jsonedit "$d/schemas/run-report.v1.schema.json" 'o["properties"]["zzz_unused_field"] = {"type": "string"}'
expect 1 "$d" check_fields "a schema property with no consumer"

# 5. check_codes defects.
d="$work/c1"; fresh "$d"; jsonedit "$d/schemas/run-report.v1.schema.json" 'o["$defs"]["refusal_code"]["enum"].remove("port-claimed")'
expect 1 "$d" check_codes "a code in the SPEC table and not in the schema"
d="$work/c2"; fresh "$d"; jsonedit "$d/schemas/run-report.v1.schema.json" 'o["$defs"]["refusal_code"]["enum"].append("invented-code")'
expect 1 "$d" check_codes "a code in the schema and not in the SPEC table"
d="$work/c3"; fresh "$d"; sed -i 's/| `port-claimed` | wait |/| `port-claimed` | maybe |/' "$d/SPEC.md"; expect 1 "$d" check_codes "a code with a class that is not wait or terminal"
d="$work/c4"; fresh "$d"; jsonedit "$d/schemas/run-report-public.v1.schema.json" 'o["$defs"]["refusal_code"]["enum"].remove("event-not-allowed")'
expect 1 "$d" check_codes "the new code missing from the public report schema"
d="$work/c5"; fresh "$d"; jsonedit "$d/schemas/run-report.v1.schema.json" 'o["$defs"]["refusal_code"]["enum"].remove("event-not-allowed")'
expect 1 "$d" check_codes "the new code missing from the full report schema"

# 6. check_tiers defects.
d="$work/t1"; fresh "$d"; sed -i 's/^    return None$/    return "untrusted"/' "$d/tools/tier_rule.py"; expect 1 "$d" check_tiers "default allow in the tier rule"
d="$work/t2"; fresh "$d"; sed -i 's/ti\["head_is_fork"\] is False/True/' "$d/tools/tier_rule.py"; expect 1 "$d" check_tiers "the fork flag ignored by the trusted rule"
d="$work/t3"; fresh "$d"; sed -i 's/^trusted_events: push, pull_request, workflow_dispatch, schedule, merge_group/trusted_events: push, pull_request, workflow_dispatch, schedule/' "$d/SPEC.md"
expect 1 "$d" check_tiers "an event dropped from the trusted list of the SPEC block"
d="$work/t4"; fresh "$d"; sed -i 's/^refused_events: pull_request_target, workflow_run, issue_comment, /refused_events: pull_request_target, workflow_run, /' "$d/SPEC.md"
expect 1 "$d" check_tiers "an event dropped from the refused list of the SPEC block"
d="$work/t5"; fresh "$d"; sed -i '/^    "issue_comment",$/d' "$d/tools/tier_rule.py"
expect 1 "$d" check_tiers "an event dropped from the refused list of the code"
d="$work/t6"; fresh "$d"; sed -i 's/^TRUSTED_EVENTS = .*/TRUSTED_EVENTS = ("push", "pull_request", "workflow_dispatch", "schedule", "merge_group", "release")/' "$d/tools/tier_rule.py"
expect 1 "$d" check_tiers "an event added to the trusted list of the code only"
d="$work/t7"; fresh "$d"; sed -i '/^             not_fork:/d' "$d/SPEC.md"; expect 1 "$d" check_tiers "the fork condition deleted from the prose rule"
d="$work/t8"; fresh "$d"; sed -i '/^    "not_fork": /d' "$d/tools/tier_rule.py"; expect 1 "$d" check_tiers "the fork predicate deleted from the code"
d="$work/t9"; fresh "$d"; sed -i 's/^`ref` and `workflow_ref` are not inputs\./`ref` and `workflow_ref` are not inputs. The event `issues` carries secrets./' "$d/SPEC.md"; expect 1 "$d" check_tiers "the prose of 6.3 names an event that the block does not"
d="$work/t10"; fresh "$d"; sed -i 's/^`ref` and `workflow_ref` are not inputs\./`ref` and `workflow_ref` are not inputs. The event `release` is refused./' "$d/SPEC.md"; expect 0 "$d" check_tiers "the prose of 6.3 names an event that the block names"
d="$work/t11"; fresh "$d"; sed -i 's/, repository_dispatch, other, unknown$/, repository_dispatch, unknown/' "$d/SPEC.md"; sed -i '/^    "other",$/d' "$d/tools/tier_rule.py"
expect 1 "$d" check_tiers "an event of the offer enum that no list of the block names"
d="$work/t12"; fresh "$d"; sed -i 's/^untrusted_events: pull_request$/untrusted_events: pull_request, push/' "$d/SPEC.md"; expect 1 "$d" check_tiers "an event added to the untrusted list of the block only"

# 7. The real tree is untouched.
n=$((n + 1))
after="$(snapshot)"
if [ "$before" != "$after" ]; then
  bad=$((bad + 1)); echo "selftest FAIL: the real tree changed during the self-test:"
  diff <(printf '%s\n' "$before") <(printf '%s\n' "$after") | head -n 20
fi

echo "selftest: $n expectations, failures: $bad"
[ "$bad" -eq 0 ] && exit 0 || exit 1
