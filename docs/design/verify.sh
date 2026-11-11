#!/usr/bin/env bash
# Verify this directory: schemas and examples, contract messages, field use, refusal codes, tier rule, document lint,
# planted-defect self-test of the guards, manifest.
# Run from anywhere. Exit 0 all checks passed, 1 a check failed, 2 could not run.
set -u
# Python checks must leave no bytecode directory behind.
export PYTHONDONTWRITEBYTECODE=1
cd "$(dirname "$0")" || exit 2
# The checks import jsonschema. Say so by name before any check runs, instead of a traceback from each.
python3 -c 'import jsonschema' 2>/dev/null || { echo "verify: needs python3 with the jsonschema module"; exit 2; }
rc=0
for check in check_schemas check_contracts check_fields check_codes check_tiers check_docs; do
  python3 "tools/$check.py"; r=$?
  [ "$r" -eq 0 ] || { echo "$check exit=$r"; rc=$(( r > rc ? r : rc )); }
done
# The guards are tested against planted defects in temporary copies of the tree (never in this tree):
# each must fail on a bad copy and pass on a good one, and the real tree must be unchanged afterwards.
bash tools/selftest.sh; r=$?
[ "$r" -eq 0 ] || { echo "selftest exit=$r"; rc=$(( r > rc ? r : rc )); }
if [ -f MANIFEST.sha256 ]; then
  sha256sum --quiet -c MANIFEST.sha256; r=$?
  [ "$r" -eq 0 ] && echo "manifest: all files match" || { echo "manifest exit=$r"; rc=1; }
else
  echo "manifest: MANIFEST.sha256 absent (built at packaging time)"
fi
echo "verify exit=$rc"
exit $rc
